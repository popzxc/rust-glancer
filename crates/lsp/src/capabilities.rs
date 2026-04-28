use tower_lsp_server::ls_types::*;

pub(crate) fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(PositionEncodingKind::UTF16),
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(false),
                change: Some(TextDocumentSyncKind::NONE),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(true),
                })),
                ..Default::default()
            },
        )),
        definition_provider: Some(OneOf::Left(true)),
        type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(false),
            trigger_characters: Some(vec![".".to_string()]),
            ..Default::default()
        }),
        document_symbol_provider: Some(OneOf::Left(true)),
        inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
            InlayHintOptions {
                resolve_provider: Some(false),
                ..Default::default()
            },
        ))),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        workspace: Some(WorkspaceServerCapabilities {
            workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                supported: Some(false), // TODO: We might in fact want to support it eventually (low prio though)
                change_notifications: Some(OneOf::Left(false)),
            }),
            file_operations: None,
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::server_capabilities;

    #[test]
    fn does_not_advertise_multi_root_workspace_support_yet() {
        let capabilities = server_capabilities();
        let workspace_folders = capabilities
            .workspace
            .and_then(|workspace| workspace.workspace_folders)
            .expect("workspace folder capability should stay explicit");

        assert_eq!(workspace_folders.supported, Some(false));
    }

    #[test]
    fn advertises_static_inlay_hint_support() {
        let capabilities = server_capabilities();
        assert!(capabilities.inlay_hint_provider.is_some());
    }
}
