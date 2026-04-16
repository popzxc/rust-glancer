use complex_crate::run_job;

mod fixtures {
    pub fn sample_input() -> &'static str {
        "  Integration Input  "
    }
}

#[tokio::test(flavor = "current_thread")]
async fn integration_job_flow() {
    let output = run_job("integration", fixtures::sample_input()).await;
    assert!(output.contains("message=integration input"));
}
