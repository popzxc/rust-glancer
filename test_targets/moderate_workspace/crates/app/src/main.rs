use std::path::PathBuf;

use moderate_workspace_app::build_report;

fn main() {
    let cwd = std::env::current_dir().expect("current directory should always exist");
    let report = build_report(&[3, 5, 8], "One two two", &PathBuf::from(cwd));
    println!("{report}");
}
