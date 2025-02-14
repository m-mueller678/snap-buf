use crate::{Node, NodePointer, SnapBuf};
use alloc::vec::Vec;
use arbitrary::Arbitrary;
use core::iter;
use core::ops::Range;

impl NodePointer {
    fn assert_minimal(&self) {
        if let Some(x) = &self.0 {
            match &**x {
                Node::Inner(x) => {
                    assert!(x.iter().any(|y| y.0.is_some()));
                    for y in x {
                        y.assert_minimal();
                    }
                }
                Node::Leaf(b) => {
                    assert!(b.iter().any(|y| *y != 0));
                }
            }
        }
    }
}

impl SnapBuf {
    fn assert_minimal(&self) {
        self.root.assert_minimal();
    }
}

#[derive(Debug, Arbitrary)]
pub enum Op {
    Write(Range<u16>),
    Clear(Range<u16>),
    Resize(u16),
}

fn cast_range(x: Range<u16>) -> Range<usize> {
    x.start as usize..x.end as usize
}

pub const MAX_TEST_OPS: usize = 250;

pub fn test(ops: &[Op]) {
    assert!(ops.len() <= MAX_TEST_OPS);
    let mut write_id = 1;
    let mut our_vec = SnapBuf::new();
    let mut std_vec = Vec::new();
    for op in ops {
        match op {
            Op::Write(range) => {
                let range = cast_range(range.clone());
                if std_vec.len() < range.end {
                    std_vec.resize(range.start, 0);
                    std_vec.extend(iter::repeat_n(write_id, range.len()));
                } else {
                    std_vec[range.clone()].fill(write_id);
                }
                our_vec.write(range.start, &std_vec[range.clone()]);
                write_id += 1;
            }
            Op::Clear(range) => {
                let range = cast_range(range.clone());
                if range.end > std_vec.iter().len() {
                    continue;
                }
                std_vec[range.clone()].fill(0);
                our_vec.clear_range(range.clone());
            }
            Op::Resize(len) => {
                let len = *len as usize;
                our_vec.resize_zero(len);
                std_vec.resize(len, 0);
            }
        }
    }
    assert_eq!(std_vec.len(), our_vec.len());
    our_vec.assert_minimal();
    itertools::assert_equal(our_vec.bytes(), std_vec.iter().copied());
}

#[test]
fn run_test() {
    test(&[])
}
