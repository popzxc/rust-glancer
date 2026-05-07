use std::{borrow::Cow, path::PathBuf};

use tower_lsp_server::{
    jsonrpc::{Error, ErrorCode, Result},
    ls_types::*,
};

use crate::{backend::ServerContext, capabilities, check::CheckConfig, config::AnalysisConfig};

pub(crate) mod text_document;
pub(crate) mod workspace;

pub(crate) async fn initialize(
    ctx: &ServerContext,
    params: InitializeParams,
) -> Result<InitializeResult> {
    let Some(root) = workspace_root(&params) else {
        return Err(Error::invalid_params(
            "rust-glancer requires a filesystem workspace root",
        ));
    };

    let check_config =
        CheckConfig::from_initialization_options(params.initialization_options.as_ref())
            .map_err(|error| Error::invalid_params(error.to_string()))?;
    let analysis_config =
        AnalysisConfig::from_initialization_options(params.initialization_options.as_ref());
    ctx.check.configure(root.clone(), check_config).await;
    ctx.engine
        .initialize(
            root,
            analysis_config.package_residency_policy,
            analysis_config.cargo_metadata_config,
        )
        .await
        .map_err(internal_error)?;

    Ok(InitializeResult {
        capabilities: capabilities::server_capabilities(),
        server_info: Some(ServerInfo {
            name: "rust-glancer".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
        offset_encoding: None,
    })
}

pub(crate) async fn initialized(ctx: &ServerContext, _: InitializedParams) {
    ctx.client
        .log_message(MessageType::INFO, "rust-glancer initialized")
        .await;
    ctx.check.launch_on_startup().await;
}

pub(crate) async fn shutdown(ctx: &ServerContext) -> Result<()> {
    ctx.check.shutdown().await;
    ctx.engine.shutdown().await.map_err(internal_error)
}

pub(crate) fn internal_error(error: anyhow::Error) -> Error {
    Error {
        code: ErrorCode::InternalError,
        message: Cow::Owned(error.to_string()),
        data: None,
    }
}

pub(crate) fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    if !uri.as_str().starts_with("file:") {
        return None;
    }

    uri.to_file_path().map(|path| path.into_owned())
}

#[allow(deprecated)]
fn workspace_root(params: &InitializeParams) -> Option<PathBuf> {
    params
        .workspace_folders
        .as_ref()
        .and_then(|folders| folders.first())
        .and_then(|folder| uri_to_path(&folder.uri))
        .or_else(|| params.root_uri.as_ref().and_then(uri_to_path))
        .or_else(|| params.root_path.as_ref().map(PathBuf::from))
}
