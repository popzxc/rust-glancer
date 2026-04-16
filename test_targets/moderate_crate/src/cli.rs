use std::fmt::Write as _;
use std::path::PathBuf;

use crate::model::Note;
use crate::text::{normalize, word_counts};

pub fn run(args: &[String], cwd: PathBuf) -> String {
    let body = args.first().map(String::as_str).unwrap_or("empty note");
    let normalized = normalize(body);
    let note = Note::new(1, normalized.clone());
    let counts = word_counts(&normalized);

    let mut output = String::new();
    let _ = writeln!(&mut output, "cwd={}", cwd.display());
    let _ = writeln!(&mut output, "note={note}");
    let _ = writeln!(&mut output, "unique_words={}", counts.len());
    output.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::cli::run;

    #[test]
    fn builds_stable_report() {
        let args = vec!["Hi hi there".to_string()];
        let output = run(&args, PathBuf::from("/tmp/example"));
        assert!(output.contains("note=#1: hi hi there"));
        assert!(output.contains("unique_words=2"));
    }
}
