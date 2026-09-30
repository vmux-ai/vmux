use std::collections::HashMap;

use tokio::sync::mpsc;
use vmux_api::protocol::ProcessId;

use crate::runtime::{Process, PtyInputWriter};

pub struct ProcessManager {
    pub processes: HashMap<ProcessId, Process>,
    wake_tx: mpsc::UnboundedSender<()>,
}

impl Default for ProcessManager {
    fn default() -> Self {
        let (wake_tx, _) = mpsc::unbounded_channel();
        Self::new(wake_tx)
    }
}

impl ProcessManager {
    pub fn new(wake_tx: mpsc::UnboundedSender<()>) -> Self {
        Self {
            processes: HashMap::new(),
            wake_tx,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_process(
        &mut self,
        id: ProcessId,
        command: String,
        args: Vec<String>,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
    ) -> Result<(ProcessId, u32), String> {
        let process = Process::new_with_wake(
            id,
            command,
            args,
            cwd,
            env,
            cols,
            rows,
            self.wake_tx.clone(),
        )?;
        let pid = process.pid;
        self.processes.insert(id, process);
        Ok((id, pid))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_process_keep_alive(
        &mut self,
        id: ProcessId,
        command: String,
        args: Vec<String>,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
    ) -> Result<(ProcessId, u32), String> {
        let created = self.create_process(id, command, args, cwd, env, cols, rows)?;
        if let Some(process) = self.processes.get_mut(&id) {
            process.set_keep_after_exit();
        }
        Ok(created)
    }

    pub fn kill_process(&mut self, id: &ProcessId) {
        if let Some(process) = self.processes.get_mut(id) {
            process.kill();
        }
    }

    pub fn poll_all(&mut self) -> Vec<ProcessId> {
        let mut exited = Vec::new();
        for (id, process) in &mut self.processes {
            if process.poll() {
                exited.push(*id);
            }
        }
        exited
    }

    pub fn reap_exited(&mut self) {
        let exited = self.poll_all();
        for id in exited {
            let keep = self
                .processes
                .get(&id)
                .is_some_and(|process| process.keep_after_exit());
            if !keep {
                self.remove_process(&id);
            }
        }
    }

    pub fn remove_process(&mut self, id: &ProcessId) {
        if let Some(mut process) = self.processes.remove(id) {
            process.kill();
        }
    }

    pub fn input_writer(&self, id: &ProcessId) -> Option<PtyInputWriter> {
        self.processes.get(id).map(Process::input_writer)
    }

    pub fn shutdown(&mut self) {
        for process in self.processes.values_mut() {
            process.kill();
        }
        self.processes.clear();
    }
}
