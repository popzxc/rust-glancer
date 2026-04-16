use complex_crate::run_job;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    init_tracing();
    let output = run_job("demo", "  Instrumented run  ").await;
    println!("{output}");
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .try_init();
}
