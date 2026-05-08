use tower_lsp_server::{
    Client,
    ls_types::{
        NumberOrString, ProgressParams, ProgressParamsValue, WorkDoneProgress, WorkDoneProgressEnd,
        notification::Progress,
    },
};

/// Small wrapper around LSP work-done progress for cargo diagnostics.
///
/// Progress is best-effort: if the client rejects token creation, diagnostics still run and publish.
#[derive(Clone, Debug)]
pub(super) struct CheckProgress {
    client: Client,
    token: NumberOrString,
}

impl CheckProgress {
    pub(super) fn new(client: Client, token: NumberOrString) -> Self {
        Self { client, token }
    }

    pub(super) fn token(&self) -> &NumberOrString {
        &self.token
    }

    pub(super) async fn begin(&self, command: String) {
        if let Err(error) = self
            .client
            .create_work_done_progress(self.token.clone())
            .await
        {
            tracing::debug!(
                error = %error,
                "failed to create cargo diagnostics progress token"
            );
            return;
        }

        let _ = self
            .client
            .progress(self.token.clone(), "Cargo diagnostics")
            .with_message(command)
            .begin()
            .await;
    }

    pub(super) async fn finish(&self, status: ProgressFinish) {
        self.client
            .send_notification::<Progress>(ProgressParams {
                token: self.token.clone(),
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(WorkDoneProgressEnd {
                    message: Some(status.message().to_string()),
                })),
            })
            .await;
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ProgressFinish {
    Cancelled,
    Failed,
    Finished,
    Superseded,
}

impl ProgressFinish {
    fn message(self) -> &'static str {
        match self {
            Self::Cancelled => "Cancelled",
            Self::Failed => "Failed",
            Self::Finished => "Finished",
            Self::Superseded => "Superseded",
        }
    }
}
