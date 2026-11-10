use std::path::Path;
use std::process::Command;

use crate::Result;

pub struct Processes {
    commands: Vec<String>,
    paths: Vec<String>,
}

impl Processes {
    pub fn capture() -> Result<Self> {
        let ps = Command::new("ps").args(["-axo", "pid=,args="]).output()?;
        if !ps.status.success() {
            return Err("Cannot inspect running processes".into());
        }
        let uid = Command::new("id").arg("-u").output()?;
        if !uid.status.success() {
            return Err("Cannot determine the current user".into());
        }
        let uid = String::from_utf8(uid.stdout)?;
        let opened = Command::new("lsof")
            .args(["-n", "-P", "-a", "-u", uid.trim(), "-d", "txt,cwd", "-Fpn"])
            .output()?;
        if !matches!(opened.status.code(), Some(0 | 1)) || !opened.stderr.is_empty() {
            return Err(format!(
                "Cannot inspect running artifacts: {}",
                String::from_utf8_lossy(&opened.stderr)
            )
            .into());
        }
        Self::parse(
            &String::from_utf8(ps.stdout)?,
            &String::from_utf8(opened.stdout)?,
            std::process::id(),
        )
    }

    pub fn parse(ps: &str, opened: &str, current_pid: u32) -> Result<Self> {
        let mut commands = Vec::new();
        for line in ps.lines() {
            let line = line.trim();
            let Some((pid, command)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            if pid.parse::<u32>()? != current_pid {
                commands.push(command.trim().to_owned());
            }
        }
        let mut paths = Vec::new();
        let mut pid = None;
        for line in opened.lines() {
            if let Some(value) = line.strip_prefix('p') {
                pid = Some(value.parse::<u32>()?);
            } else if let Some(value) = line.strip_prefix('n')
                && pid.is_some()
                && pid != Some(current_pid)
            {
                paths.push(value.to_owned());
            }
        }
        if paths.is_empty() {
            return Err("Cannot inspect running artifacts: lsof returned no open paths".into());
        }
        Ok(Self { commands, paths })
    }

    pub fn uses(&self, path: &Path) -> bool {
        let value = path.to_string_lossy();
        for command in &self.commands {
            if command.contains(value.as_ref()) {
                return true;
            }
        }
        for opened in &self.paths {
            if opened == value.as_ref() || Path::new(opened).starts_with(path) {
                return true;
            }
        }
        false
    }

    #[cfg(test)]
    pub fn idle() -> Self {
        Self {
            commands: Vec::new(),
            paths: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Processes;

    #[test]
    fn protects_executable_when_arguments_do_not_include_its_path() {
        let processes = Processes::parse(
            "12 vmux_desktop\n99 storage --target /tmp/output\n",
            "p12\nn/tmp/output/debug/Vmux Dev.app/Contents/MacOS/vmux_desktop\np99\nn/tmp/output\n",
            99,
        )
        .unwrap();
        assert!(processes.uses(Path::new("/tmp/output/debug")));
        assert!(!processes.uses(Path::new("/tmp/output/release")));
    }

    #[test]
    fn refuses_an_empty_artifact_snapshot() {
        assert!(Processes::parse("12 vmux_desktop", "", 99).is_err());
    }
}
