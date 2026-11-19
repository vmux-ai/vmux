use std::path::Path;

pub struct BrowserCommand(String);

impl BrowserCommand {
    pub fn for_launch(binary: &Path, source: &str, anchor: vmux_api::ProcessId) -> Self {
        Self(format!(
            "{} open --source {} --anchor {} %s",
            Self::quote(&binary.to_string_lossy()),
            Self::quote(source),
            Self::quote(&anchor.to_string())
        ))
    }

    pub fn into_string(self) -> String {
        self.0
    }

    fn quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn browser_launcher_preserves_quoted_paths_and_url_arguments() {
        let fixture = tempfile::tempdir().unwrap();
        let binary = fixture.path().join("Vmux Dev 'test'.app").join("vmux");
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$VMUX_BROWSER_TEST_ARGS\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = fixture.path().join("arguments");
        let source = "source with 'quotes'";
        let url = "https://example.test/auth?state=$(printf%20unsafe)&challenge=%2F%2B#`test`";
        let anchor = vmux_api::ProcessId::new();
        let status = std::process::Command::new("python3")
            .args(["-c", "import os,sys,webbrowser; b=webbrowser.get(os.environ['BROWSER']); sys.exit(0 if b.open(sys.argv[1]) else 1)"])
            .arg(url)
            .env("BROWSER", BrowserCommand::for_launch(&binary, source, anchor).into_string())
            .env("VMUX_BROWSER_TEST_ARGS", &output)
            .status()
            .expect("Python is required to verify the provider browser launcher");
        assert!(status.success());
        let arguments = std::fs::read_to_string(output).unwrap();
        assert_eq!(
            arguments.lines().collect::<Vec<_>>(),
            vec![
                "open".to_string(),
                "--source".into(),
                source.into(),
                "--anchor".into(),
                anchor.to_string(),
                url.into()
            ]
        );
    }
}
