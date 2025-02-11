use smallvec::SmallVec;
use std::cmp::Ordering;
use std::mem;
use std::ops::Range;
use std::sync::Arc;

#[derive(Debug)]
pub struct CowVec {
    size: usize,
    root_height: usize,
    root: NodePointer,
}

const LEAF_SIZE: usize = if cfg!(feature = "test") { 32 } else { 4000 };
const INNER_SIZE: usize = if cfg!(feature = "test") { 4 } else { 500 };

#[cfg(feature = "test")]
pub mod test;

#[derive(Clone, Debug)]
enum CowVecNode {
    Inner([NodePointer; INNER_SIZE]),
    Leaf([u8; LEAF_SIZE]),
}

#[derive(Clone, Debug)]
struct NodePointer(Option<Arc<CowVecNode>>);

impl NodePointer {
    fn children(&self) -> Option<&[NodePointer; INNER_SIZE]> {
        match &**(self.0.as_ref()?) {
            CowVecNode::Inner(x) => Some(x),
            CowVecNode::Leaf(_) => None,
        }
    }

    fn get_mut(&mut self, height: usize) -> &mut CowVecNode {
        let arc = self.0.get_or_insert_with(|| {
            Arc::new({
                if height == 0 {
                    CowVecNode::Leaf([0; LEAF_SIZE])
                } else {
                    CowVecNode::Inner(array_init::array_init(|_| NodePointer(None)))
                }
            })
        });
        Arc::make_mut(arc)
    }

    fn set_range(&mut self, height: usize, start: usize, values: &[u8]) {
        debug_assert!(start < tree_size(height));
        match self.get_mut(height) {
            CowVecNode::Inner(children) => {
                let child_size = tree_size(height - 1);
                let first_child = start / child_size;
                let last_child = ((start + values.len() - 1) / child_size).min(children.len() - 1);
                for child in first_child..=last_child {
                    let child_offset = child * child_size;
                    if child_offset <= start {
                        children[child].set_range(height - 1, start - child_offset, values)
                    } else {
                        let values = &values[child_offset - start..];
                        children[child].set_range(
                            height - 1,
                            start.saturating_sub(child_offset),
                            values,
                        )
                    }
                }
            }
            CowVecNode::Leaf(bytes) => {
                let write_len = (bytes.len() - start).min(values.len());
                bytes[start..start + write_len].copy_from_slice(&values[..write_len]);
            }
        }
    }

    fn clear_range(&mut self, height: usize, range: Range<usize>) {
        let start = range.start;
        let end = range.end;
        let self_size = tree_size(height);
        debug_assert!(start < self_size);
        if start == 0 && end >= self_size || self.0.is_none() {
            self.0 = None;
            return;
        }
        match self.get_mut(height) {
            CowVecNode::Inner(children) => {
                let child_size = tree_size(height - 1);
                let first_child = start / child_size;
                let last_child = ((end - 1) / child_size).min(children.len() - 1);
                for child in first_child..=last_child {
                    let child_offset = child * child_size;
                    children[child].clear_range(
                        height - 1,
                        start.saturating_sub(child_offset)..end - child_offset,
                    );
                }
                if children.first().unwrap().0.is_none()
                    && children.last().unwrap().0.is_none()
                    && children.iter().all(|x| x.0.is_none())
                {
                    self.0 = None;
                }
            }
            CowVecNode::Leaf(bytes) => {
                let write_end = range.end.min(bytes.len());
                bytes[start..write_end].fill(0);
                if bytes[0] == 0 && *bytes.last().unwrap() == 0 && bytes.iter().all(|x| *x == 0) {
                    self.0 = None;
                }
            }
        }
    }
}

const fn const_tree_size(height: usize) -> usize {
    if height == 0 {
        LEAF_SIZE
    } else {
        INNER_SIZE * const_tree_size(height - 1)
    }
}

fn tree_size(height: usize) -> usize {
    const_tree_size(height)
}

impl CowVec {
    pub fn new() -> Self {
        CowVec {
            root_height: 0,
            size: 0,
            root: NodePointer(None),
        }
    }

    pub fn resize(&mut self, new_size: usize) {
        match new_size.cmp(&self.size) {
            Ordering::Less => {
                self.root
                    .clear_range(self.root_height, new_size..tree_size(self.root_height));
                self.size = new_size;
            }
            Ordering::Equal => {}
            Ordering::Greater => {
                while tree_size(self.root_height) < new_size {
                    if self.root.0.is_some() {
                        let new_root = Arc::new(CowVecNode::Inner(array_init::array_init(|x| {
                            if x == 0 {
                                self.root.clone()
                            } else {
                                NodePointer(None)
                            }
                        })));
                        self.root = NodePointer(Some(new_root.clone()));
                    }
                    self.root_height += 1;
                }
                self.size = new_size;
            }
        }
    }

    /// Write data at specified offset, growing if necessary.
    pub fn write(&mut self, offset: usize, data: &[u8]) {
        let write_end = offset + data.len();
        if self.size < write_end {
            self.resize(write_end);
        }
        if data.is_empty() {
            return;
        }
        self.root.set_range(self.root_height, offset, data);
    }

    pub fn len(&self) -> usize {
        self.size
    }

    /// Clears data in range, possibly freeing memory.
    ///
    /// # Panics
    /// Panics if range end is `range.end` > `self.len()`.
    pub fn clear_range(&mut self, range: Range<usize>) {
        assert!(range.end <= self.size);
        if range.is_empty() {
            return;
        }
        self.root.clear_range(self.root_height, range);
    }

    /// Clears all data and sets length to 0.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    fn iter_nodes_pre_order(&self) -> impl Iterator<Item = (&NodePointer, usize)> {
        struct IterStack<'a> {
            stack_end_height: usize,
            stack: SmallVec<[&'a [NodePointer]; 5]>,
        }

        fn split_first_in_place<'x, 's, T>(x: &'x mut &'s [T]) -> &'s T {
            let (first, rest) = mem::take(x).split_first().unwrap();
            *x = rest;
            first
        }

        impl<'a> Iterator for IterStack<'a> {
            type Item = (&'a NodePointer, usize);

            fn next(&mut self) -> Option<Self::Item> {
                let visit_now = loop {
                    let last_level = self.stack.last_mut()?;
                    if last_level.is_empty() {
                        self.stack.pop();
                        self.stack_end_height += 1;
                    } else {
                        break split_first_in_place(last_level);
                    }
                };
                let ret = (visit_now, self.stack_end_height);
                if let Some(children) = visit_now.children() {
                    self.stack.push(children);
                    self.stack_end_height -= 1;
                }
                Some(ret)
            }
        }

        let mut stack = SmallVec::new();
        stack.push(std::slice::from_ref(&self.root));
        IterStack {
            stack_end_height: self.root_height,
            stack,
        }
    }

    pub fn chunks(&self) -> impl Iterator<Item = &[u8]> {
        let mut emitted = 0;
        self.iter_nodes_pre_order()
            .flat_map(|(node, height)| {
                let zero_leaf = &[0u8; LEAF_SIZE];
                match node.0.as_deref() {
                    None => {
                        let leaf_count = INNER_SIZE.pow(height as u32);
                        std::iter::repeat_n(zero_leaf, leaf_count)
                    }
                    Some(CowVecNode::Inner(_)) => std::iter::repeat_n(zero_leaf, 0),
                    Some(CowVecNode::Leaf(b)) => std::iter::repeat_n(b, 1),
                }
            })
            .map(move |x| {
                let emit = (self.size - emitted).min(x.len());
                emitted += emit;
                &x[..emit]
            })
            .filter(|x| !x.is_empty())
    }

    pub fn bytes(&self) -> impl Iterator<Item = u8> + '_ {
        self.chunks().flat_map(|x| x.iter().copied())
    }
}
