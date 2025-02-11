#![no_main]

use std::io::Read;
use libfuzzer_sys::{fuzz_target, Corpus};

fuzz_target!(|data: Vec<Op>|->Corpus{
    test(&data)
});


use cow_vec::CowVec;
use std::ops::Range;
use arbitrary::{Arbitrary, Unstructured};

#[derive(Debug)]
enum Op {
    Write(Range<usize>),
    Clear(Range<usize>),
}

impl<'a> Arbitrary<'a> for Op {
    fn arbitrary(u: &mut Unstructured) -> arbitrary::Result<Self> {
        let (is_write,range) :(bool,Range<u16>)=Arbitrary::arbitrary(u)?;
        if range.start>range.end{
            return Err(arbitrary::Error::IncorrectFormat);
        }
        let range = range.start as usize .. range.end as usize;
        if is_write {
            Ok(Op::Write(range))
        }else{
            Ok(Op::Clear(range))
        }
    }
}

fn test(ops:&[Op]) ->Corpus {
    if ops.len()>250{
        return Corpus::Reject
    }
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
    itertools::assert_equal(cow_vec.bytes(),std_vec.iter().copied());
    Corpus::Keep
}
