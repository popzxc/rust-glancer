use std::path::PathBuf;

use moderate_crate::cli::run;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().expect("current directory should always exist");
    let output = run(&args, PathBuf::from(cwd));
    println!("{output}");
}
