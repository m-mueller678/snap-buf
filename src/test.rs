use crate::{Node, NodePointer, SnapBuf};
use alloc::vec;
use alloc::vec::Vec;
use arbitrary::Arbitrary;
use core::iter;
use core::ops::Range;
use extend::ext;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

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

macro_rules! define_op {
    (
        fn $rand_ident:ident();
        fn $std_vec:ident();
        $(
            fn $name:ident($($arg:ident:$Arg:ty),*){
                $(filter($filter:expr);)?
                $(let $extra_name:pat = $extra_val:expr;)*
                ($($arg2:expr),*)
            }
        )*
    ) => {
        #[derive(Debug, Arbitrary)]
        #[allow(non_camel_case_types)]
        pub enum Op {
            $($name{$($arg:$Arg),*}),*
        }

        impl Op{
            fn check(&self)->bool{
                match self{
                    $(
                    Op::$name{$($arg),*} =>{
                        $(return $filter;)?
                    }
                    )*
                }
                true
            }

            fn apply(self,$std_vec:&mut Vec<u8>,our:&mut SnapBuf,$rand_ident:&mut SmallRng)->Result<(),()>{
                match self{
                    $(
                    Op::$name{$($arg),*} =>{
                        $(let $extra_name = $extra_val;)*
                        $std_vec.$name($($arg2),*);
                        our.$name($($arg2),*);
                    }
                    )*
                }
                Ok(())
            }
        }
    };
}

fn random_bytes(rng: &mut SmallRng, len: usize, allow_zeros: bool) -> Vec<u8> {
    fn dense(rng: &mut SmallRng, len: usize) -> Vec<u8> {
        let mut b = vec![0u8; len];
        rng.fill(&mut b[..]);
        for b in &mut b {
            if *b == 0 {
                *b = 1;
            }
        }
        b
    }
    fn sparse(rng: &mut SmallRng, len: usize) -> Vec<u8> {
        let mut b = vec![0u8; len];
        let mut written = 0;
        while written < len {
            let skip_to = rng.random_range(written..=len);
            let write_to = rng.random_range(skip_to..=len);
            rng.fill(&mut b[skip_to..write_to]);
            written = write_to;
        }
        b
    }
    if allow_zeros && rng.random() {
        sparse(rng, len)
    } else {
        dense(rng, len)
    }
}

define_op!(
    fn rng();
    fn std_vec();
    fn resize(len: u16, value: u8) {
        (len as usize, value)
    }
    fn fill_range(range: Range<u16>, value: u8) {
        filter(range.start <= range.end && *value != 0);
        (cast_range(range.clone()), value)
    }
    fn write(range: Range<u16>) {
        filter(range.start <= range.end);
        let data = random_bytes(rng, range.len(), false);
        (range.start as usize, &*data)
    }
    fn write_with_zeros(range: Range<u16>) {
        filter(range.start <= range.end);
        let data = random_bytes(rng, range.len(), true);
        (range.start as usize, &*data)
    }
    fn clear() {
        ()
    }
    fn truncate(len: u16) {
        (len as usize)
    }
    fn extend_from_slice(len: u16) {
        let data = random_bytes(
            rng,
            (len as usize).min(u16::MAX as usize - std_vec.len()),
            true,
        );
        (&data)
    }
    fn extend(len: u16) {
        let data = random_bytes(
            rng,
            (len as usize).min(u16::MAX as usize - std_vec.len()),
            true,
        );
        (data.iter().copied())
    }
    fn clear_range(range: Range<u16>) {
        filter(range.start <= range.end);
        let () = {
            if std_vec.len() < range.end as usize {
                return Err(());
            }
        };
        (cast_range(range.clone()))
    }
);

#[ext]
impl Vec<u8> {
    fn fill_range(&mut self, range: Range<usize>, value: u8) {
        if range.end < self.len() {
            self[range].fill(value);
        } else {
            self.resize(range.start, 0);
            self.extend(iter::repeat_n(value, range.len()));
        }
    }

    fn clear_range(&mut self, range: Range<usize>) {
        self[range].fill(0);
    }

    fn write(&mut self, offset: usize, data: &[u8]) {
        if offset + data.len() < self.len() {
            self[offset..offset + data.len()].copy_from_slice(data);
        } else {
            self.resize(offset, 0);
            self.extend_from_slice(data);
        }
    }

    fn write_with_zeros(&mut self, offset: usize, data: &[u8]) {
        self.write(offset, data);
    }
}

fn cast_range(x: Range<u16>) -> Range<usize> {
    x.start as usize..x.end as usize
}

pub const MAX_TEST_OPS: usize = 250;

pub fn test(ops: Vec<Op>) -> Result<(), ()> {
    if ops.len() > MAX_TEST_OPS {
        return Err(());
    }
    for op in &ops {
        if !op.check() {
            return Err(());
        }
    }
    let mut our_vec = SnapBuf::new();
    let mut std_vec = Vec::new();
    let rng = &mut SmallRng::seed_from_u64(1);
    for op in ops {
        op.apply(&mut std_vec, &mut our_vec, rng)?;
    }
    assert_eq!(std_vec.len(), our_vec.len());
    our_vec.assert_minimal();
    itertools::assert_equal(our_vec.bytes(), std_vec.iter().copied());
    Ok(())
}

#[test]
fn run_test() {
    test(vec![Op::extend_from_slice { len: 2 }]).unwrap()
}
