use complex_workspace_app::{run_once, summarize};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    init_tracing();
    let outputs = run_once().await;
    println!("{}", summarize(&outputs));
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .try_init();
}
