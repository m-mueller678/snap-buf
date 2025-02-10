use std::num::NonZero;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

pub struct CowVec {
    size: usize,
    height: usize,
    root: NodePointer,
}

const LEAF_SIZE: usize = 4000;
const INNER_SIZE: usize = 500;

enum CowVecNode {
    Inner([NodePointer; INNER_SIZE]),
    Leaf([u8; LEAF_SIZE]),
}

#[derive(Clone)]
struct NodePointer(Option<Arc<CowVecNode>>);

impl NodePointer {
    fn get_mut(&mut self, height: usize) -> &mut CowVecNode {
        let arc = self.0.get_or_insert_with(|| {
            Arc::new({
                if height == 0 {
                    CowVecNode::Leaf([0; LEAF_SIZE])
                } else {
                    CowVecNode::Inner(
                        NonZero::new(height as u8).unwrap(),
                        array_init::array_init(|_| NodePointer(None)),
                    )
                }
            })
        });
        Arc::make_mut(arc)
    }
    fn set_range(&mut self, height: usize, start: usize, values: &[u8]) {
        match self.get_mut(height) {
            CowVecNode::Inner(children) => {
                let child_size = tree_size(height - 1);
                let first_child = start / child_size;
                let last_child = (start + values.len() - 1) / child_size;
                for child in first_child..=last_child {
                    let child_offset = first_child * child_size;
                    if child_offset <= start {
                        children[child].set_range(start - child_offset, height - 1, values)
                    } else {
                        let values = values[child_offset - start..];
                        children[child].set_range(start - child_offset, height - 1, values)
                    }
                }
            }
            CowVecNode::Leaf(_) => {}
        }
    }

    fn child_index(height: usize, index: usize) -> usize {
        index / tree_size(height - 1)
    }
}

const fn const_tree_size(height: usize) -> usize {
    if height == 0 {
        LEAF_SIZE
    } else {
        INNER_SIZE * tree_size(height - 1)
    }
}

fn tree_size(height: usize) -> usize {
    const_tree_size(height)
}

impl CowVec {
    pub fn new() -> Self {
        CowVec {
            size: 0,
            root: NodePointer(None),
        }
    }
}
