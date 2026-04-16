use complex_workspace_app::{run_once, summarize};

mod fixtures {
    pub fn expect_prefix() -> &'static str {
        "phase=done"
    }
}

#[tokio::test(flavor = "current_thread")]
async fn integration_flow() {
    let outputs = run_once().await;
    let summary = summarize(&outputs);
    assert!(summary.starts_with(fixtures::expect_prefix()));
}
