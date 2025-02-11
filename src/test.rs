use crate::{CowVec, CowVecNode, NodePointer};
use arbitrary::{Arbitrary, Unstructured};
use std::ops::Range;

impl NodePointer {
    fn assert_minimal(&self) {
        if let Some(x) = &self.0 {
            match &**x {
                CowVecNode::Inner(x) => {
                    assert!(x.iter().any(|y| y.0.is_some()));
                    for y in x {
                        y.assert_minimal();
                    }
                }
                CowVecNode::Leaf(b) => {
                    assert!(b.iter().any(|y| *y != 0));
                }
            }
        }
    }
}

impl CowVec {
    fn assert_minimal(&self) {
        self.root.assert_minimal();
    }
}

#[derive(Debug)]
pub enum Op {
    Write(Range<usize>),
    Clear(Range<usize>),
}

impl<'a> Arbitrary<'a> for Op {
    fn arbitrary(u: &mut Unstructured) -> arbitrary::Result<Self> {
        let (is_write, range): (bool, Range<u16>) = Arbitrary::arbitrary(u)?;
        if range.start > range.end {
            return Err(arbitrary::Error::IncorrectFormat);
        }
        let range = range.start as usize..range.end as usize;
        if is_write {
            Ok(Op::Write(range))
        } else {
            Ok(Op::Clear(range))
        }
    }
}

pub const MAX_TEST_OPS: usize = 250;

pub fn test(ops: &[Op]) {
    assert!(ops.len() <= MAX_TEST_OPS);
    let mut write_id = 1;
    let mut cow_vec = CowVec::new();
    let mut std_vec = Vec::new();
    for op in ops {
        match op {
            Op::Write(range) => {
                if std_vec.len() < range.end {
                    std_vec.resize(range.start, 0);
                    std_vec.extend(std::iter::repeat_n(write_id, range.len()));
                } else {
                    std_vec[range.clone()].fill(write_id);
                }
                cow_vec.write(range.start, &std_vec[range.clone()]);
                write_id += 1;
            }
            Op::Clear(range) => {
                if range.end > std_vec.iter().len() {
                    continue;
                }
                std_vec[range.clone()].fill(0);
                cow_vec.clear_range(range.clone());
            }
        }
    }
    cow_vec.assert_minimal();
    itertools::assert_equal(cow_vec.bytes(), std_vec.iter().copied());
}
