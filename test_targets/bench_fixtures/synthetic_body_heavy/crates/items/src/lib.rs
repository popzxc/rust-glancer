#![allow(dead_code)]

pub mod group_00;
pub mod group_01;
pub mod group_02;
pub mod group_03;
pub mod group_04;
pub mod group_05;
pub mod group_06;
pub mod group_07;
pub mod group_08;
pub mod group_09;

pub trait FoldValue {
    fn fold_value(&self, seed: i64) -> i64;
}

pub fn combined_seed(mut seed: i64) -> i64 {
    seed = group_00::entry(seed);
    seed = group_01::entry(seed);
    seed = group_02::entry(seed);
    seed = group_03::entry(seed);
    seed = group_04::entry(seed);
    seed = group_05::entry(seed);
    seed = group_06::entry(seed);
    seed = group_07::entry(seed);
    seed = group_08::entry(seed);
    seed = group_09::entry(seed);
    seed
}
