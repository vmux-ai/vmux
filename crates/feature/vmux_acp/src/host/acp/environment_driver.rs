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

    fn env(key: &str, value: &str) -> (String, String) {
        (key.to_string(), value.to_string())
    }

    #[test]
    fn login_environment_overrides_registry_environment() {
        let base = vec![env("MISTRAL_API_KEY", ""), env("KEEP", "1")];
        let login = vec![
            env("MISTRAL_API_KEY", "real-key"),
            env("PATH", "/login/bin"),
        ];
        let environment = AcpEnvironment::build(base, &login, None).into_inner();

        assert!(environment.contains(&env("MISTRAL_API_KEY", "real-key")));
        assert!(environment.contains(&env("KEEP", "1")));
        assert!(environment.contains(&env("PATH", "/login/bin")));
    }

    #[test]
    fn managed_binary_precedes_login_path() {
        let login = vec![env("PATH", "/login/bin")];
        let environment =
            AcpEnvironment::build(Vec::new(), &login, Some("/managed/node/bin".to_string()))
                .into_inner();

        assert!(environment.contains(&env("PATH", "/managed/node/bin:/login/bin")));
    }

    #[test]
    fn managed_binary_uses_environment_path() {
        let environment = AcpEnvironment::build(
            vec![env("PATH", "/from/login")],
            &[],
            Some("/managed".to_string()),
        )
        .into_inner();

        assert!(environment.contains(&env("PATH", "/managed:/from/login")));
    }

    #[test]
    fn catalog_environment_keeps_explicit_values_and_drops_login_credentials() {
        let base = vec![env("MISTRAL_API_KEY", "configured")];
        let login = vec![
            env("MISTRAL_API_KEY", "inherited"),
            env("AWS_SECRET_ACCESS_KEY", "secret"),
            env("HOME", "/home/test"),
            env("PATH", "/login/bin"),
        ];
        let environment = AcpEnvironment::catalog(base, &login, None).into_inner();

        assert!(environment.contains(&env("MISTRAL_API_KEY", "configured")));
        assert!(environment.contains(&env("HOME", "/home/test")));
        assert!(environment.contains(&env("PATH", "/login/bin")));
        assert!(
            !environment
                .iter()
                .any(|(key, _)| key == "AWS_SECRET_ACCESS_KEY")
        );
        assert!(!environment.contains(&env("MISTRAL_API_KEY", "inherited")));
    }
}
