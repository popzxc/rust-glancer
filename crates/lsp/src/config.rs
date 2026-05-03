use rg_project::{PackageResidencyPolicy, ProjectBuildOptions};
use tower_lsp_server::ls_types::LSPAny;

/// Analysis configuration sent by the LSP client during initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnalysisConfig {
    pub(crate) build_options: ProjectBuildOptions,
}

impl AnalysisConfig {
    pub(crate) fn from_initialization_options(options: Option<&LSPAny>) -> Self {
        let default = Self::default();
        let package_residency_policy = options
            .and_then(LSPAny::as_object)
            .and_then(|options| {
                options
                    .get("cache")
                    .and_then(LSPAny::as_object)
                    .and_then(|cache| cache.get("packageResidency"))
            })
            .and_then(LSPAny::as_str)
            .and_then(PackageResidencyPolicy::from_config_name)
            .unwrap_or(default.build_options.package_residency_policy);

        Self {
            build_options: ProjectBuildOptions {
                package_residency_policy,
                ..default.build_options
            },
        }
    }
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            build_options: ProjectBuildOptions {
                // LSP optimizes for low steady-state memory by default. Workspace and local path
                // dependencies are the packages users are most likely to edit by hand, so they
                // remain resident while registry/git dependencies can be offloaded.
                package_residency_policy: PackageResidencyPolicy::WorkspaceAndPathDepsResident,
                ..ProjectBuildOptions::default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use rg_project::PackageResidencyPolicy;
    use tower_lsp_server::ls_types::LSPAny;

    use super::AnalysisConfig;

    #[test]
    fn defaults_to_workspace_and_path_dependency_residency() {
        let config = AnalysisConfig::from_initialization_options(None);

        assert_eq!(
            config.build_options.package_residency_policy,
            PackageResidencyPolicy::WorkspaceAndPathDepsResident,
        );
    }

    #[test]
    fn parses_cache_residency_policy() {
        let options = object([(
            "cache",
            object([(
                "packageResidency",
                LSPAny::String("all-resident".to_string()),
            )]),
        )]);

        let config = AnalysisConfig::from_initialization_options(Some(&options));

        assert_eq!(
            config.build_options.package_residency_policy,
            PackageResidencyPolicy::AllResident,
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
