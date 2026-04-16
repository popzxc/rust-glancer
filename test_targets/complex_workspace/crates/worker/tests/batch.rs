use complex_workspace_common::WorkItem;
use complex_workspace_worker::execute_batch;

#[tokio::test(flavor = "current_thread")]
async fn executes_batch_in_order() {
    let items = vec![WorkItem::new(1, "a"), WorkItem::new(2, "b")];
    let out = execute_batch(&items).await;
    assert_eq!(out, vec!["1:a", "2:b"]);
}
