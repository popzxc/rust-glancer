use std::path::PathBuf;

use moderate_crate::cli::run;

#[test]
fn smoke_test() {
    let output = run(&["One two two".to_string()], PathBuf::from("/workspace"));
    assert!(output.contains("unique_words=2"));
}
