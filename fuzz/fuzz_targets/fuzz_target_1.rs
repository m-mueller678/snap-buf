#![no_main]

use libfuzzer_sys::{fuzz_target, Corpus};
use snap_buf::test::{test, Op};

fuzz_target!(|ops: Vec<Op>|->Corpus{
    if test(ops).is_ok(){
        Corpus::Keep
    }else{
        Corpus::Reject
    }
});
