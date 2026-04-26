use std::path::PathBuf;

pub fn test_file(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test_targets")
        .join(path)
}
