use std::path::Path;

use moderate_workspace_app::build_report;

#[test]
fn report_smoke_test() {
    let report = build_report(&[10, 20], "line one\nline two", Path::new("/workspace"));
    assert!(report.contains("avg=15.0"));
}
