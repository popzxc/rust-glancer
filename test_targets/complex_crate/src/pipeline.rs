use std::time::Duration;

use tracing::{debug, info};

use crate::text;

// =============================================================================
// Pipeline Construction
// =============================================================================
// We model a tiny async pipeline with two phases: collecting numbers and
// generating a summary string. The phases are intentionally simple, but they
// exercise async/await, tracing instrumentation, and inter-module calls.

#[tracing::instrument]
pub async fn collect_numbers(limit: usize) -> Vec<usize> {
    let capped = limit.min(8);
    let mut numbers = Vec::with_capacity(capped + 1);

    for idx in 0..=capped {
        tokio::time::sleep(Duration::from_millis(1)).await;
        numbers.push(idx * idx);
    }

    info!(count = numbers.len(), "collected values");
    numbers
}

#[tracing::instrument(skip(numbers))]
pub async fn summarize(numbers: &[usize], message: &str) -> String {
    tokio::time::sleep(Duration::from_millis(1)).await;

    let total: usize = numbers.iter().sum();
    let normalized = text::trim_and_lower(message);
    debug!(total, normalized, "summarized values");

    format!("items={},total={},message={normalized}", numbers.len(), total)
}

#[cfg(test)]
mod tests {
    #[tokio::test(flavor = "current_thread")]
    async fn collects_square_sequence() {
        let values = crate::pipeline::collect_numbers(3).await;
        assert_eq!(values, vec![0, 1, 4, 9]);
    }
}
