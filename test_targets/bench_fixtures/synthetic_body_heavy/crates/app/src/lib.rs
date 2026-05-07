#![allow(dead_code)]

pub fn run(seed: i64) -> i64 {
    bench_body::run_all(seed) + bench_items::combined_seed(seed)
}
