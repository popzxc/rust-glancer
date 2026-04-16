use complex_workspace_common::WorkItem;
use complex_workspace_worker::execute_batch;
use tracing::info;

// =============================================================================
// App Orchestration
// =============================================================================
// The app crate wires common data structures with async worker execution.
// This is intentionally compact but demonstrates how a binary-facing crate can
// coordinate cross-crate async behavior while exposing a testable library API.

pub async fn run_once() -> Vec<String> {
    let items = vec![WorkItem::new(10, "scan"), WorkItem::new(11, "index")];
    let outputs = execute_batch(&items).await;
    info!(count = outputs.len(), "run completed");
    outputs
}

pub fn summarize(outputs: &[String]) -> String {
    complex_workspace_common::status_line!("done", outputs.len())
}

#[cfg(test)]
mod tests {
    #[tokio::test(flavor = "current_thread")]
    async fn runs_once_and_summarizes() {
        let outputs = crate::run_once().await;
        assert_eq!(outputs.len(), 2);
        assert_eq!(crate::summarize(&outputs), "phase=done value=2");
    }
}
