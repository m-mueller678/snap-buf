#![no_main]

use libfuzzer_sys::{fuzz_target, Corpus};
use shared_buffer::test::{test, Op, MAX_TEST_OPS};

fuzz_target!(|ops: Vec<Op>|->Corpus{
    if ops.len()>MAX_TEST_OPS{
        Corpus::Reject
    }else{
        test(&ops);
        Corpus::Keep
    }
});
