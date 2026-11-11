use std::io;
use std::path::Path;
use std::process::Command;

use crate::Settings;

pub(crate) struct Sandbox;

impl Sandbox {
    pub(crate) fn command(
        helper: &Path,
        settings: &Settings,
        configuration: &Path,
    ) -> io::Result<Command> {
        let helper = helper.canonicalize()?;
        let profile = format!(
            "(version 1)\n(allow default)\n(deny file-read-data)\n(deny file-write*)\n(allow file-read-data (literal \"/\") (subpath \"/System\") (subpath \"/usr\") (subpath \"/Library\") (subpath \"/opt/homebrew\") (subpath \"/dev\") (subpath {data}) (subpath {project}) (literal {binary}))\n(allow file-write* (subpath {data}) (subpath {project}) (literal \"/dev/null\"))\n(deny file-read-data (literal {key}))\n",
            data = Self::literal(&settings.directory)?,
            project = Self::literal(&settings.shared_directory)?,
            binary = Self::literal(&helper)?,
            key = Self::literal(&settings.directory.join("ssh-key"))?,
        );
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command.env("TMPDIR", &settings.directory);
        command
            .arg("-p")
            .arg(profile)
            .arg(helper)
            .arg("native")
            .arg(configuration);
        Ok(command)
    }

    fn literal(path: &Path) -> io::Result<String> {
        let path = path.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "sandbox path must be UTF-8")
        })?;
        serde_json::to_string(path).map_err(io::Error::other)
    }
}
