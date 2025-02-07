use std::num::NonZero;
use std::sync::atomic::AtomicUsize;

pub struct CowVec {}

const PAGE_SIZE: usize = 4096;

#[repr(C)]
struct CowVecNode {
    ref_count: AtomicUsize,
}

enum CowVecInner {
    Inner(NonZero<u8>, [u8; PAGE_SIZE - size_of::<AtomicUsize>() - 1]),
    Leaf([u8; PAGE_SIZE - size_of::<AtomicUsize>() - 1]),
}

//
// #[repr(C)]
// struct CowVecLeaf{
//     ref_count:AtomicUsize,
//     height:u8,
//     entries:[T;PAGE_SIZE - size_of::<AtomicUsize>() - 1],
// }
