#![no_main]

use libfuzzer_sys::{fuzz_target, Corpus};
use snap_buf::test::{test, Op, MAX_TEST_OPS};

fuzz_target!(|ops: Vec<Op>|->Corpus{
    if test(ops).is_ok(){
        Corpus::Keep
    }else{
        Corpus::Reject
    }
});
