use std::path::Path;

use moderate_workspace_math::{mean, sum};
use moderate_workspace_text::{collect_unique_words, first_line};

pub fn build_report(numbers: &[i64], body: &str, cwd: &Path) -> String {
    let total = sum(numbers);
    let avg = mean(numbers).unwrap_or(0.0);
    let first = first_line(body);
    let unique = collect_unique_words(body).len();

    format!(
        "cwd={} total={} avg={avg:.1} first_line={first} unique_words={unique}",
        cwd.display(),
        total
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::build_report;

    #[test]
    fn includes_basic_metrics() {
        let report = build_report(&[1, 2, 3], "Alpha alpha\nBeta", Path::new("/tmp/test"));
        assert!(report.contains("total=6"));
        assert!(report.contains("unique_words=2"));
    }
}
