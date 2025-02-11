use cow_vec::CowVec;
use std::ops::Range;

enum Op {
    Write(Range<usize>),
    Clear(Range<usize>),
}

fn test() {
    let ops = vec![Op::Write(0..12)];
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
                cow_vec.write(range.start, &std_vec[range]);
                write_id += 1;
            }
            Op::Clear(range) => {
                if range.end > std_vec.iter().len() {
                    continue;
                }
                std_vec[range.clone()].fill(0);
                cow_vec.clear_range(range);
            }
        }
    }
    cow_vec.assert_minimal();
    itertools::assert_equal();
}
