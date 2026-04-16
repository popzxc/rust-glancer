use std::time::Duration;

use complex_workspace_common::WorkItem;
use tracing::{debug, info};

#[tracing::instrument(skip(item))]
pub async fn execute(item: &WorkItem) -> String {
    tokio::time::sleep(Duration::from_millis(2)).await;
    let payload = format!("{}:{}", item.id, item.label);
    info!(id = item.id, "executed item");
    debug!(payload, "produced payload");
    payload
}

pub async fn execute_batch(items: &[WorkItem]) -> Vec<String> {
    let mut outputs = Vec::with_capacity(items.len());
    for item in items {
        outputs.push(execute(item).await);
    }
    outputs
}

#[cfg(test)]
mod tests {
    use complex_workspace_common::WorkItem;

    #[tokio::test(flavor = "current_thread")]
    async fn executes_single_item() {
        let output = crate::execute(&WorkItem::new(1, "alpha")).await;
        assert_eq!(output, "1:alpha");
    }
}
