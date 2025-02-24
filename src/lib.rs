#![no_std]
//! A [SnapBuf] is like a `Vec<u8>` with cheap snapshotting using copy on write.
//!
//! Internally, the data is broken up into segments that are organized in a tree structure.
//! Only modified subtrees are cloned, so buffers with only little differences can share most of their memory.
//! Moreover, subtrees which contain only zeros take up no memory.

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

use alloc::sync::Arc;
use core::cmp::Ordering;
use core::ops::Range;
use core::{iter, mem, slice};
use smallvec::SmallVec;

/// A copy on write buffer.
///
/// See the crate level documentation for more details.
#[derive(Debug, Clone)]
pub struct SnapBuf {
    size: usize,
    root_height: usize,
    root: NodePointer,
}

const LEAF_SIZE: usize = if cfg!(feature = "test") { 32 } else { 4000 };
const INNER_SIZE: usize = if cfg!(feature = "test") { 4 } else { 500 };

#[cfg(feature = "test")]
pub mod test;

#[cfg(feature = "std")]
mod cursor;

#[cfg(feature = "std")]
pub use cursor::SnapBufCursor;

#[derive(Clone, Debug)]
enum Node {
    Inner([NodePointer; INNER_SIZE]),
    Leaf([u8; LEAF_SIZE]),
}

#[derive(Clone, Debug)]
struct NodePointer(Option<Arc<Node>>);

macro_rules! deconstruct_range{
    {$start:ident .. $end:ident = $range:expr,$height:expr} => {
         let $start = $range.start;
        let $end = $range.end;
         // assert range overlaps
        debug_assert!($start < tree_size($height) as isize);
        debug_assert!($end > 0);
    }
}

fn range_all<T, const C: usize>(x: &[T; C], mut f: impl FnMut(&T) -> bool) -> bool {
    let last = f(x.last().unwrap());
    last && x[0..C - 1].iter().all(f)
}

impl NodePointer {
    fn children(&self) -> Option<&[NodePointer; INNER_SIZE]> {
        match &**(self.0.as_ref()?) {
            Node::Inner(x) => Some(x),
            Node::Leaf(_) => None,
        }
    }

    fn get_mut(&mut self, height: usize) -> &mut Node {
        let arc = self.0.get_or_insert_with(|| {
            Arc::new({
                if height == 0 {
                    Node::Leaf([0; LEAF_SIZE])
                } else {
                    Node::Inner(array_init::array_init(|_| NodePointer(None)))
                }
            })
        });
        Arc::make_mut(arc)
    }

    fn set_range<const FREE_ZEROS: bool>(&mut self, height: usize, start: isize, values: &[u8]) {
        deconstruct_range!(start..end = start .. start + values.len() as isize ,height);
        match self.get_mut(height) {
            Node::Inner(children) => {
                for (child_offset, child) in
                    Self::affected_children(children, height - 1, start..end)
                {
                    child.set_range::<FREE_ZEROS>(height - 1, start - child_offset, values);
                }
                if FREE_ZEROS && range_all(children, |c| c.0.is_none()) {
                    self.0 = None;
                }
            }
            Node::Leaf(bytes) => {
                let (src, dst) = if start < 0 {
                    (&values[-start as usize..], &mut bytes[..])
                } else {
                    (values, &mut bytes[start as usize..])
                };
                let len = src.len().min(dst.len());
                dst[..len].copy_from_slice(&src[..len]);
                if FREE_ZEROS && range_all(bytes, |b| *b == 0) {
                    self.0 = None;
                }
            }
        }
    }

    fn affected_children(
        children: &mut [NodePointer; INNER_SIZE],
        child_height: usize,
        range: Range<isize>,
    ) -> impl Iterator<Item = (isize, &mut NodePointer)> {
        let start = range.start.max(0) as usize;
        let child_size = tree_size(child_height);
        children
            .iter_mut()
            .enumerate()
            .skip(start / child_size)
            .map(move |(i, c)| ((i * child_size) as isize, c))
            .take_while(move |(offset, _)| (*offset) < range.end)
    }

    fn fill_range(&mut self, height: usize, range: Range<isize>, value: u8) {
        deconstruct_range!(start..end=range,height);
        match self.get_mut(height) {
            Node::Inner(children) => {
                for (child_offset, child) in
                    Self::affected_children(children, height - 1, range.clone())
                {
                    child.fill_range(height - 1, start - child_offset..end - child_offset, value);
                }
            }
            Node::Leaf(bytes) => {
                let write_start = start.max(0) as usize;
                let write_end = (end as usize).min(bytes.len());
                bytes[write_start..write_end].fill(value);
            }
        }
    }

    fn clear_range(&mut self, height: usize, range: Range<isize>) {
        deconstruct_range!(start..end = range,height);
        if start <= 0 && end as usize >= tree_size(height) || self.0.is_none() {
            self.0 = None;
            return;
        }
        match self.get_mut(height) {
            Node::Inner(children) => {
                for (child_offset, child) in
                    Self::affected_children(children, height - 1, range.clone())
                {
                    child.clear_range(height - 1, start - child_offset..end - child_offset);
                }
                if range_all(children, |c| c.0.is_none()) {
                    self.0 = None;
                }
            }
            Node::Leaf(bytes) => {
                let write_start = start.max(0) as usize;
                let write_end = (end as usize).min(bytes.len());
                bytes[write_start..write_end].fill(0);
                if range_all(bytes, |b| *b == 0) {
                    self.0 = None;
                }
            }
        }
    }

    fn put_leaf(&mut self, height: usize, offset: usize, leaf: NodePointer) {
        match self.get_mut(height) {
            Node::Inner(children) => {
                let range = offset as isize..offset as isize + 1;
                let (co, c) = Self::affected_children(children, height - 1, range)
                    .next()
                    .unwrap();
                c.put_leaf(height - 1, offset - co as usize, leaf);
            }
            Node::Leaf(_) => {
                debug_assert_eq!(offset, 0);
                *self = leaf;
            }
        }
    }

    fn locate_leaf(
        &mut self,
        height: usize,
        offset: usize,
    ) -> Option<(usize, &mut [u8; LEAF_SIZE])> {
        self.0.as_ref()?;
        match self.get_mut(height) {
            Node::Inner(children) => {
                let range = offset as isize..offset as isize + 1;
                let (co, c) = Self::affected_children(children, height - 1, range)
                    .next()
                    .unwrap();
                c.locate_leaf(height - 1, offset - co as usize)
            }
            Node::Leaf(x) => Some((offset, x)),
        }
    }

    fn locate_leaf_ref(&self, heigth: usize, offset: usize) -> (usize, &[u8; LEAF_SIZE]) {
        if let Some(x) = &self.0 {
            match &**x {
                Node::Inner(children) => {
                    let child_size = tree_size(heigth - 1);
                    children[offset / child_size].locate_leaf_ref(heigth - 1, offset % child_size)
                }
                Node::Leaf(data) => (offset, data),
            }
        } else {
            (offset % LEAF_SIZE, &ZERO_LEAF)
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

impl Default for SnapBuf {
    fn default() -> Self {
        Self::new()
    }
}

static ZERO_LEAF: [u8; LEAF_SIZE] = [0; LEAF_SIZE];

impl SnapBuf {
    /// Creates an empty buffer.
    pub fn new() -> Self {
        Self {
            root_height: 0,
            size: 0,
            root: NodePointer(None),
        }
    }

    fn shrink(&mut self, new_len: usize) {
        self.root.clear_range(
            self.root_height,
            new_len as isize..tree_size(self.root_height) as isize,
        );
        self.size = new_len;
    }

    fn grow_height_until(&mut self, min_size: usize) {
        while tree_size(self.root_height) < min_size {
            if self.root.0.is_some() {
                let new_root = Arc::new(Node::Inner(array_init::array_init(|x| {
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
    }

    fn grow_zero(&mut self, new_len: usize) {
        self.grow_height_until(new_len);
        self.size = new_len;
    }

    /// Resizes the buffer, to the given length.
    ///
    /// If `new_len` is greater than `len`, the new space in the buffer is filled with copies of value.
    /// This is more efficient if `value == 0`.
    #[inline]
    pub fn resize(&mut self, new_len: usize, value: u8) {
        match new_len.cmp(&self.size) {
            Ordering::Less => {
                self.shrink(new_len);
            }
            Ordering::Equal => {}
            Ordering::Greater => {
                let old_len = self.size;
                self.grow_zero(new_len);
                if value != 0 {
                    self.fill_range(old_len..new_len, value);
                }
            }
        }
    }

    /// Shortens the buffer, keeping the first `new_len` bytes and discarding the rest.
    ///
    /// If `new_len` is greater or equal to the buffer’s current length, this has no effect.
    pub fn truncate(&mut self, new_len: usize) {
        if new_len < self.size {
            self.shrink(new_len);
        }
    }

    /// Fill the given range with copies of value.
    ///
    /// This is equivalent to calling [write](Self::write) with a slice filled with value.
    /// Calling this with `value = 0` is not guaranteed to free up the zeroed segments,
    /// use [clear_range](Self::clear_range) if that is required.
    pub fn fill_range(&mut self, range: Range<usize>, value: u8) {
        if self.size < range.end {
            self.grow_zero(range.end);
        }
        if range.is_empty() {
            return;
        }
        let range = range.start as isize..range.end as isize;
        self.root.fill_range(self.root_height, range, value);
    }

    pub fn read(&self, offset: usize) -> &[u8] {
        if offset == self.size {
            return &[];
        }
        assert!(offset < self.size);
        let max_len = self.size - offset;
        let (offset, leaf) = self.root.locate_leaf_ref(self.root_height, offset);
        let data = &leaf[offset..];
        &data[..data.len().min(max_len)]
    }

    /// Writes data at the specified offset.
    ///
    /// If this extends past the current end of the buffer, the buffer is automatically resized.
    /// If offset is larger than the current buffer length, the space between the current buffer
    /// end and the written region is filled with zeros.
    ///
    /// Zeroing parts of the buffer using this method is not guaranteed to free up the zeroed segments,
    /// use [write_with_zeros](Self::write_with_zeros) if that is required.
    pub fn write(&mut self, offset: usize, data: &[u8]) {
        self.write_inner::<false>(offset, data);
    }

    /// Like [write](Self::write), but detects and frees newly zeroed segments in the buffer.
    pub fn write_with_zeros(&mut self, offset: usize, data: &[u8]) {
        self.write_inner::<true>(offset, data);
    }

    fn write_inner<const FREE_ZEROS: bool>(&mut self, offset: usize, data: &[u8]) {
        let write_end = offset + data.len();
        if self.size < write_end {
            self.resize(write_end, 0);
        }
        if data.is_empty() {
            return;
        }
        self.root
            .set_range::<FREE_ZEROS>(self.root_height, offset as isize, data);
    }

    /// Returns `true` if the buffer length is zero.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the length of the buffer, the number of bytes it contains.
    ///
    /// The memory footprint of the buffer may be much smaller than this due to omission of zero segments and sharing with other buffers.
    pub fn len(&self) -> usize {
        self.size
    }

    /// Fill a range with zeros, freeing memory if possible.
    ///
    /// # Panics
    /// Panics if range end is `range.end` > `self.len()`.
    pub fn clear_range(&mut self, range: Range<usize>) {
        assert!(range.end <= self.size);
        if range.is_empty() {
            return;
        }
        self.root
            .clear_range(self.root_height, range.start as isize..range.end as isize);
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

        #[allow(clippy::needless_lifetimes)]
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
        stack.push(slice::from_ref(&self.root));
        IterStack {
            stack_end_height: self.root_height,
            stack,
        }
    }

    /// Returns an iterator over the byte slices constituting the buffer.
    ///
    /// The returned slices may overlap.
    pub fn chunks(&self) -> impl Iterator<Item = &[u8]> {
        let mut emitted = 0;
        self.iter_nodes_pre_order()
            .flat_map(|(node, height)| match node.0.as_deref() {
                None => {
                    let leaf_count = INNER_SIZE.pow(height as u32);
                    iter::repeat_n(&ZERO_LEAF, leaf_count)
                }
                Some(Node::Inner(_)) => iter::repeat_n(&ZERO_LEAF, 0),
                Some(Node::Leaf(b)) => iter::repeat_n(b, 1),
            })
            .map(move |x| {
                let emit = (self.size - emitted).min(x.len());
                emitted += emit;
                &x[..emit]
            })
            .filter(|x| !x.is_empty())
    }

    /// Returns an iterator over the buffer.
    pub fn iter(&self) -> impl Iterator<Item = &u8> {
        self.chunks().flat_map(|x| x.iter())
    }

    #[doc(hidden)]
    pub fn bytes(&self) -> impl Iterator<Item = u8> + '_ {
        self.iter().copied()
    }

    pub fn extend_from_slice(&mut self, data: &[u8]) {
        self.write_with_zeros(self.size, data)
    }
}

impl Extend<u8> for SnapBuf {
    fn extend<T: IntoIterator<Item = u8>>(&mut self, iter: T) {
        fn generate_leaf(
            start_at: usize,
            iter: &mut impl Iterator<Item = u8>,
        ) -> (usize, NodePointer) {
            let mut consumed = start_at;
            let first_non_zero = loop {
                if let Some(x) = iter.next() {
                    consumed += 1;
                    if x != 0 {
                        break x;
                    }
                } else {
                    return (consumed, NodePointer(None));
                }
                if consumed == LEAF_SIZE {
                    return (LEAF_SIZE, NodePointer(None));
                }
            };
            let mut leaf = Arc::new(Node::Leaf([0u8; LEAF_SIZE]));
            let leaf_mut = if let Node::Leaf(x) = Arc::get_mut(&mut leaf).unwrap() {
                x
            } else {
                unreachable!()
            };
            leaf_mut[consumed - 1] = first_non_zero;
            while consumed < LEAF_SIZE {
                if let Some(x) = iter.next() {
                    leaf_mut[consumed] = x;
                    consumed += 1;
                } else {
                    break;
                }
            }
            (consumed, NodePointer(Some(leaf)))
        }

        let it = &mut iter.into_iter();
        if self.size < tree_size(self.root_height) {
            if let Some((offset, first_leaf)) = self.root.locate_leaf(self.root_height, self.size) {
                for i in offset..LEAF_SIZE {
                    let Some(x) = it.next() else { return };
                    first_leaf[i] = x;
                    self.size += 1;
                }
                assert_eq!(self.size % LEAF_SIZE, 0);
            }
        } else {
            assert_eq!(self.size % LEAF_SIZE, 0);
        }
        loop {
            let in_leaf_offset = self.size % LEAF_SIZE;
            let (consumed, leaf) = generate_leaf(in_leaf_offset, it);
            let old_size = self.size;
            self.size = old_size - in_leaf_offset + consumed;
            self.grow_height_until(self.size);
            if leaf.0.is_some() {
                self.root
                    .put_leaf(self.root_height, old_size - in_leaf_offset, leaf);
            }
            if consumed < LEAF_SIZE {
                return;
            }
            assert_eq!(self.size % LEAF_SIZE, 0);
        }
    }
}

impl FromIterator<u8> for SnapBuf {
    fn from_iter<T: IntoIterator<Item = u8>>(iter: T) -> Self {
        let mut iter = iter.into_iter();
        let mut ret = Self::new();
        ret.extend(&mut iter);
        ret
    }
}
