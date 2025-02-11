bin_dir="$(rustc --print target-libdir | sed 's:/lib$:/bin:')"
target="$(rustc -vV | sed -n 's|host: ||p')"

RUSTFLAGS="-C instrument-coverage" cargo fuzz coverage fuzz_target_1

"$bin_dir"/llvm-cov show -Xdemangler=rustfilt target/"$target"/coverage/"$target"/release/fuzz_target_1\
  -instr-profile=fuzz/coverage/fuzz_target_1/coverage.profdata\
  -show-line-counts-or-regions\
  -show-instantiations