pub mod pipeline;
pub mod text;

#[macro_export]
macro_rules! label_result {
    ($label:expr, $value:expr) => {
        format!("[{}] {}", $label, $value)
    };
}

pub async fn run_job(job_name: &str, input: &str) -> String {
    let numbers = pipeline::collect_numbers(5).await;
    let summary = pipeline::summarize(&numbers, input).await;
    crate::label_result!(job_name, summary)
}

#[cfg(test)]
mod tests {
    #[tokio::test(flavor = "current_thread")]
    async fn run_job_labels_result() {
        let output = crate::run_job("job-a", "  TEXT ").await;
        assert!(output.starts_with("[job-a]"));
    }
}
