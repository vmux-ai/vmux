use std::ffi::OsString;
use std::path::PathBuf;

use crate::Result;
use crate::environment::Environment;
use crate::execution::Execution;

pub struct Repository(pub PathBuf);

impl Repository {
    pub fn worktrees(&self) -> Result<Vec<Record>> {
        let environment = Environment::current();
        let mut command = environment.command("git", &self.0);
        command.args(["worktree", "list", "--porcelain", "-z"]);
        let output = Execution::new(command).output(None)?;
        let mut records = Record::parse(&output.stdout)?;
        let mut command = environment.command("git", &self.0);
        command.args(["rev-parse", "--path-format=absolute", "--git-common-dir"]);
        let common = Execution::new(command).output(None)?;
        let common = PathBuf::from(std::str::from_utf8(&common.stdout)?.trim());
        let mut command = environment.command("git", &self.0);
        command.args(["config", "--local", "--get", "core.worktree"]);
        let setting = Execution::new(command).output_any(None)?;
        if setting.status.success() {
            let root = common
                .join(std::str::from_utf8(&setting.stdout)?.trim())
                .canonicalize()?;
            for record in &mut records {
                if record.path == common {
                    record.path = root.clone();
                }
            }
        }
        Ok(records)
    }
}

pub struct Record {
    pub path: PathBuf,
    pub branch: Option<OsString>,
}

impl Record {
    fn parse(bytes: &[u8]) -> Result<Vec<Self>> {
        let text = std::str::from_utf8(bytes)?;
        let mut records = Vec::new();
        let mut path = None;
        let mut branch = None;
        for field in text.split('\0') {
            if let Some(value) = field.strip_prefix("worktree ") {
                path = Some(PathBuf::from(value));
            } else if let Some(value) = field.strip_prefix("branch refs/heads/") {
                branch = Some(value.into());
            } else if field.is_empty()
                && let Some(path) = path.take()
            {
                records.push(Self {
                    path,
                    branch: branch.take(),
                });
            }
        }
        Ok(records)
    }
}
