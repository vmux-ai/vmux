pub(super) struct AcpEnvironment(Vec<(String, String)>);

impl AcpEnvironment {
    pub(super) fn build(
        mut base: Vec<(String, String)>,
        login_env: &[(String, String)],
        path_prepend: Option<String>,
    ) -> Self {
        base.extend(login_env.iter().cloned());
        let mut environment = Self(base);
        environment.deduplicate();
        environment.prepend_path(path_prepend);
        environment
    }

    pub(super) fn catalog(
        base: Vec<(String, String)>,
        login_env: &[(String, String)],
        path_prepend: Option<String>,
    ) -> Self {
        let login_env = login_env
            .iter()
            .filter(|(key, _)| CatalogVariable::allowed(key))
            .cloned()
            .collect::<Vec<_>>();
        Self::build(base, &login_env, path_prepend)
    }

    pub(super) fn into_inner(self) -> Vec<(String, String)> {
        self.0
    }

    fn prepend_path(&mut self, prepend: Option<String>) {
        let Some(directory) = prepend else {
            return;
        };
        let existing = self
            .0
            .iter()
            .find(|(key, _)| key == "PATH")
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var("PATH").ok())
            .filter(|value| !value.is_empty());
        let path = match existing {
            Some(existing) => format!("{directory}:{existing}"),
            None => directory,
        };
        self.0.retain(|(key, _)| key != "PATH");
        self.0.push(("PATH".to_string(), path));
    }

    fn deduplicate(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut environment = Vec::with_capacity(self.0.len());
        for (key, value) in std::mem::take(&mut self.0).into_iter().rev() {
            if seen.insert(key.clone()) {
                environment.push((key, value));
            }
        }
        environment.reverse();
        self.0 = environment;
    }
}

struct CatalogVariable;

impl CatalogVariable {
    fn allowed(key: &str) -> bool {
        matches!(
            key,
            "HOME"
                | "PATH"
                | "TMPDIR"
                | "LANG"
                | "USER"
                | "LOGNAME"
                | "SHELL"
                | "XDG_CONFIG_HOME"
                | "XDG_CACHE_HOME"
                | "XDG_DATA_HOME"
                | "XDG_STATE_HOME"
        ) || key.starts_with("LC_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_environment_overrides_registry_environment() {
        let base = vec![
            ("MISTRAL_API_KEY".into(), String::new()),
            ("KEEP".into(), "1".into()),
        ];
        let login = vec![
            ("MISTRAL_API_KEY".into(), "real-key".into()),
            ("PATH".into(), "/login/bin".into()),
        ];
        let environment = AcpEnvironment::build(base, &login, None).into_inner();

        assert!(environment.contains(&("MISTRAL_API_KEY".into(), "real-key".into())));
        assert!(environment.contains(&("KEEP".into(), "1".into())));
        assert!(environment.contains(&("PATH".into(), "/login/bin".into())));
    }

    #[test]
    fn managed_binary_precedes_login_path() {
        let login = vec![("PATH".into(), "/login/bin".into())];
        let environment =
            AcpEnvironment::build(Vec::new(), &login, Some("/managed/node/bin".to_string()))
                .into_inner();

        assert!(environment.contains(&("PATH".into(), "/managed/node/bin:/login/bin".into())));
    }

    #[test]
    fn managed_binary_uses_environment_path() {
        let environment = AcpEnvironment::build(
            vec![("PATH".into(), "/from/login".into())],
            &[],
            Some("/managed".to_string()),
        )
        .into_inner();

        assert!(environment.contains(&("PATH".into(), "/managed:/from/login".into())));
    }

    #[test]
    fn catalog_environment_keeps_explicit_values_and_drops_login_credentials() {
        let base = vec![("MISTRAL_API_KEY".into(), "configured".into())];
        let login = vec![
            ("MISTRAL_API_KEY".into(), "inherited".into()),
            ("AWS_SECRET_ACCESS_KEY".into(), "secret".into()),
            ("HOME".into(), "/home/test".into()),
            ("PATH".into(), "/login/bin".into()),
        ];
        let environment = AcpEnvironment::catalog(base, &login, None).into_inner();

        assert!(environment.contains(&("MISTRAL_API_KEY".into(), "configured".into())));
        assert!(environment.contains(&("HOME".into(), "/home/test".into())));
        assert!(environment.contains(&("PATH".into(), "/login/bin".into())));
        assert!(
            !environment
                .iter()
                .any(|(key, _)| key == "AWS_SECRET_ACCESS_KEY")
        );
        assert!(!environment.contains(&("MISTRAL_API_KEY".into(), "inherited".into())));
    }
}
