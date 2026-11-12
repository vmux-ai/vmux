use std::io;

use tokio::process::{Child, Command};
use vmux_spawn::{Descriptors, ProcessGroup};

pub(super) struct AgentProcess {
    pub(super) child: Child,
    group: ProcessGroup,
}

impl AgentProcess {
    pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
        command.kill_on_drop(true);
        std::os::unix::process::CommandExt::process_group(command.as_std_mut(), 0);
        Descriptors::isolate(command.as_std_mut())?;
        let child = command.spawn()?;
        let group = ProcessGroup::of(
            child
                .id()
                .ok_or_else(|| io::Error::other("missing child PID"))?,
        )?;
        Ok(Self { child, group })
    }

    pub(super) async fn stop(&mut self) {
        let _ = self.group.kill();
        let _ = self.child.kill().await;
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        let _ = self.group.kill();
    }
}

#[cfg(test)]
mod tests {
    use std::process::Stdio;
    use std::time::Duration;

    use tokio::io::AsyncReadExt;

    use super::*;

    #[tokio::test]
    async fn an_exited_agent_does_not_leave_descendants_holding_its_output_open() {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "sleep 30 & printf 'ready\\n'"])
            .stdout(Stdio::piped());
        let mut process = AgentProcess::spawn(&mut command).unwrap();
        let mut output = process.child.stdout.take().unwrap();
        assert!(process.child.wait().await.unwrap().success());
        assert!(process.child.id().is_none());
        drop(process);
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), output.read_to_end(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bytes, b"ready\n");
    }

    #[tokio::test]
    async fn aborting_an_agent_task_closes_output_inherited_by_its_descendants() {
        let (ready, started) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut command = Command::new("/bin/sh");
            command
                .args(["-c", "sleep 30 & printf 'ready\\n'; wait"])
                .stdout(Stdio::piped());
            let mut process = AgentProcess::spawn(&mut command).unwrap();
            ready.send(process.child.stdout.take().unwrap()).unwrap();
            let _ = process.child.wait().await;
        });
        let mut output = started.await.unwrap();
        let mut ready = [0; 6];
        tokio::time::timeout(Duration::from_secs(2), output.read_exact(&mut ready))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&ready, b"ready\n");
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), output.read_to_end(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        assert!(bytes.is_empty());
    }
}
