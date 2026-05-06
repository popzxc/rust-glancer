use tower_lsp_server::ls_types::LSPAny;

/// Cargo diagnostics configuration sent by the VS Code client during initialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CheckConfig {
    pub(crate) on_startup: bool,
    pub(crate) on_save: bool,
    pub(crate) command: String,
    pub(crate) arguments: Vec<String>,
}

impl CheckConfig {
    pub(crate) fn from_initialization_options(options: Option<&LSPAny>) -> Self {
        let Some(check) = options
            .and_then(LSPAny::as_object)
            .and_then(|options| options.get("check"))
            .and_then(LSPAny::as_object)
        else {
            return Self::default();
        };

        let on_startup = check
            .get("onStartup")
            .and_then(LSPAny::as_bool)
            .unwrap_or_default();
        let on_save = check
            .get("onSave")
            .and_then(LSPAny::as_bool)
            .unwrap_or_default();
        let command = check
            .get("command")
            .and_then(LSPAny::as_str)
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .unwrap_or("check")
            .to_string();
        let arguments = check
            .get("arguments")
            .and_then(LSPAny::as_array)
            .map(|arguments| {
                arguments
                    .iter()
                    .filter_map(LSPAny::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_else(|| vec!["--workspace".to_string(), "--all-targets".to_string()]);

        Self {
            on_startup,
            on_save,
            command,
            arguments,
        }
    }

    pub(crate) fn user_facing_command(&self) -> String {
        let mut parts = vec![
            "cargo".to_string(),
            self.command.clone(),
            "--message-format=json".to_string(),
        ];
        parts.extend(self.arguments.iter().cloned());
        parts.join(" ")
    }
}

impl Default for CheckConfig {
    fn default() -> Self {
        Self {
            on_startup: false,
            on_save: false,
            command: "check".to_string(),
            arguments: vec!["--workspace".to_string(), "--all-targets".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CheckConfig;
    use tower_lsp_server::ls_types::LSPAny;

    #[test]
    fn defaults_to_disabled_cargo_check() {
        let config = CheckConfig::from_initialization_options(None);

        assert!(!config.on_startup);
        assert!(!config.on_save);
        assert_eq!(
            config.user_facing_command(),
            "cargo check --message-format=json --workspace --all-targets"
        );
    }

    #[test]
    fn parses_client_check_configuration() {
        let options = object([(
            "check",
            object([
                ("onStartup", LSPAny::Bool(true)),
                ("onSave", LSPAny::Bool(true)),
                ("command", LSPAny::String("clippy".to_string())),
                (
                    "arguments",
                    LSPAny::Array(vec![
                        LSPAny::String("--workspace".to_string()),
                        LSPAny::String("--all-targets".to_string()),
                        LSPAny::String("--".to_string()),
                        LSPAny::String("-Dwarnings".to_string()),
                    ]),
                ),
            ]),
        )]);

        let config = CheckConfig::from_initialization_options(Some(&options));

        assert!(config.on_startup);
        assert!(config.on_save);
        assert_eq!(config.command, "clippy");
        assert_eq!(
            config.arguments,
            ["--workspace", "--all-targets", "--", "-Dwarnings"]
        );
        assert_eq!(
            config.user_facing_command(),
            "cargo clippy --message-format=json --workspace --all-targets -- -Dwarnings"
        );
    }

    fn object<const N: usize>(entries: [(&str, LSPAny); N]) -> LSPAny {
        let mut map = match LSPAny::Object(Default::default()) {
            LSPAny::Object(map) => map,
            _ => unreachable!("constructed object should be an object"),
        };
        for (key, value) in entries {
            map.insert(key.to_string(), value);
        }
        LSPAny::Object(map)
    }
}
