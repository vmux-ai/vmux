use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

pub(super) struct ProcessSnapshot {
    pub samples: HashMap<u32, ProcSample>,
    pub local: Vec<(u32, LocalProcess)>,
}

pub(super) struct ProcSample {
    pub parent: Option<u32>,
    pub cpu: f32,
    pub mem: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct LocalProcess {
    pub shell: String,
    pub cwd: String,
    pub uptime_secs: u64,
}

#[cfg(target_os = "macos")]
pub(super) struct ProcessSampler {
    sample_poll: Timer,
    discovery_poll: Timer,
    discovered: bool,
    local: HashMap<u32, MacLocalProcess>,
    counters: HashMap<u32, ProcessCounter>,
}

#[cfg(target_os = "macos")]
impl Default for ProcessSampler {
    fn default() -> Self {
        Self {
            sample_poll: Timer::from_seconds(2.0, TimerMode::Repeating),
            discovery_poll: Timer::from_seconds(300.0, TimerMode::Repeating),
            discovered: false,
            local: HashMap::new(),
            counters: HashMap::new(),
        }
    }
}

#[cfg(target_os = "macos")]
impl ProcessSampler {
    pub fn refresh(
        &mut self,
        delta: std::time::Duration,
        immediate: bool,
        service_roots: impl IntoIterator<Item = u32>,
    ) -> Option<ProcessSnapshot> {
        self.sample_poll.tick(delta);
        self.discovery_poll.tick(delta);
        if !immediate && !self.sample_poll.just_finished() {
            return None;
        }
        if immediate || !self.discovered || self.discovery_poll.just_finished() {
            self.discover();
        }
        self.sample_poll.reset();
        Some(self.snapshot(service_roots))
    }

    fn discover(&mut self) {
        self.discovered = true;
        self.discovery_poll.reset();
        self.local.clear();
        for pid in MacProcess::all() {
            let Some(process) = MacProcess::new(pid) else {
                continue;
            };
            let name = process.name();
            if LocalProcess::matches(&name, "") {
                self.local.insert(pid, MacLocalProcess::read(&process));
            }
        }
    }

    #[allow(deprecated)]
    fn snapshot(&mut self, service_roots: impl IntoIterator<Item = u32>) -> ProcessSnapshot {
        let mut pids = self.local.keys().copied().collect::<HashSet<_>>();
        pids.extend(service_roots);
        let mut pending = pids.iter().copied().collect::<Vec<_>>();
        while let Some(pid) = pending.pop() {
            for child in MacProcess::children(pid) {
                if pids.insert(child) {
                    pending.push(child);
                }
            }
        }

        let now = unsafe { libc::mach_absolute_time() };
        let mut samples = HashMap::new();
        let mut local = Vec::new();
        let mut stale = Vec::new();
        for pid in &pids {
            let Some(process) = MacProcess::new(*pid) else {
                continue;
            };
            let Some(task) = process.task() else {
                continue;
            };
            let total = task.pti_total_user.saturating_add(task.pti_total_system);
            let cpu = self
                .counters
                .entry(*pid)
                .or_insert_with(|| ProcessCounter::new(process.started_at(), now, total))
                .update(process.started_at(), now, total);
            samples.insert(
                *pid,
                ProcSample {
                    parent: process.parent(),
                    cpu,
                    mem: task.pti_resident_size,
                },
            );
            if let Some(details) = self.local.get(pid) {
                if details.started_at == process.started_at() {
                    local.push((*pid, details.snapshot()));
                } else {
                    stale.push(*pid);
                }
            }
        }
        for pid in stale {
            self.local.remove(&pid);
        }
        self.counters.retain(|pid, _| pids.contains(pid));
        ProcessSnapshot { samples, local }
    }
}

#[cfg(target_os = "macos")]
struct MacLocalProcess {
    started_at: u64,
    shell: String,
    cwd: String,
}

#[cfg(target_os = "macos")]
impl MacLocalProcess {
    fn read(process: &MacProcess) -> Self {
        let executable = process.executable();
        Self {
            started_at: process.started_at(),
            shell: if executable.is_empty() {
                process.name()
            } else {
                executable
            },
            cwd: process.cwd(),
        }
    }

    fn snapshot(&self) -> LocalProcess {
        LocalProcess {
            shell: self.shell.clone(),
            cwd: self.cwd.clone(),
            uptime_secs: unix_time().saturating_sub(self.started_at),
        }
    }
}

#[cfg(target_os = "macos")]
struct ProcessCounter {
    started_at: u64,
    sampled_at: u64,
    cpu_time: u64,
}

#[cfg(target_os = "macos")]
impl ProcessCounter {
    fn new(started_at: u64, sampled_at: u64, cpu_time: u64) -> Self {
        Self {
            started_at,
            sampled_at,
            cpu_time,
        }
    }

    fn update(&mut self, started_at: u64, sampled_at: u64, cpu_time: u64) -> f32 {
        if self.started_at != started_at {
            *self = Self::new(started_at, sampled_at, cpu_time);
            return 0.0;
        }
        let elapsed = sampled_at.saturating_sub(self.sampled_at);
        let used = cpu_time.saturating_sub(self.cpu_time);
        self.sampled_at = sampled_at;
        self.cpu_time = cpu_time;
        if elapsed == 0 {
            return 0.0;
        }
        used as f32 * 100.0 / elapsed as f32
    }
}

#[cfg(target_os = "macos")]
struct MacProcess {
    pid: u32,
    info: libc::proc_bsdinfo,
}

#[cfg(target_os = "macos")]
impl MacProcess {
    fn new(pid: u32) -> Option<Self> {
        let mut info = unsafe { std::mem::zeroed::<libc::proc_bsdinfo>() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
        let read = unsafe {
            libc::proc_pidinfo(
                pid as i32,
                libc::PROC_PIDTBSDINFO,
                0,
                &mut info as *mut _ as *mut libc::c_void,
                size,
            )
        };
        (read == size).then_some(Self { pid, info })
    }

    fn all() -> Vec<u32> {
        let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
        Self::read_pid_list(count, |buffer, size| unsafe {
            libc::proc_listallpids(buffer, size)
        })
    }

    fn children(pid: u32) -> Vec<u32> {
        let count = unsafe { libc::proc_listchildpids(pid as i32, std::ptr::null_mut(), 0) };
        Self::read_pid_list(count, |buffer, size| unsafe {
            libc::proc_listchildpids(pid as i32, buffer, size)
        })
    }

    fn read_pid_list(count: i32, read: impl FnOnce(*mut libc::c_void, i32) -> i32) -> Vec<u32> {
        if count <= 0 {
            return Vec::new();
        }
        let mut pids = vec![0u32; count as usize];
        let size = (pids.len() * std::mem::size_of::<u32>()) as i32;
        let read = read(pids.as_mut_ptr().cast(), size);
        if read <= 0 {
            return Vec::new();
        }
        pids.truncate((read as usize).min(pids.len()));
        pids.retain(|pid| *pid != 0);
        pids
    }

    fn task(&self) -> Option<libc::proc_taskinfo> {
        let mut task = unsafe { std::mem::zeroed::<libc::proc_taskinfo>() };
        let size = std::mem::size_of::<libc::proc_taskinfo>() as i32;
        let read = unsafe {
            libc::proc_pidinfo(
                self.pid as i32,
                libc::PROC_PIDTASKINFO,
                0,
                &mut task as *mut _ as *mut libc::c_void,
                size,
            )
        };
        (read == size).then_some(task)
    }

    fn parent(&self) -> Option<u32> {
        (self.info.pbi_ppid != 0).then_some(self.info.pbi_ppid)
    }

    fn started_at(&self) -> u64 {
        self.info.pbi_start_tvsec
    }

    fn name(&self) -> String {
        let name = Self::c_chars(&self.info.pbi_name);
        if name.is_empty() {
            Self::c_chars(&self.info.pbi_comm)
        } else {
            name
        }
    }

    fn executable(&self) -> String {
        let mut buffer = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let read = unsafe {
            libc::proc_pidpath(
                self.pid as i32,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
            )
        };
        if read <= 0 {
            return String::new();
        }
        buffer.truncate(read as usize);
        String::from_utf8_lossy(&buffer).into_owned()
    }

    fn cwd(&self) -> String {
        let mut paths = unsafe { std::mem::zeroed::<libc::proc_vnodepathinfo>() };
        let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as i32;
        let read = unsafe {
            libc::proc_pidinfo(
                self.pid as i32,
                libc::PROC_PIDVNODEPATHINFO,
                0,
                &mut paths as *mut _ as *mut libc::c_void,
                size,
            )
        };
        if read != size {
            return String::new();
        }
        let path = paths.pvi_cdir.vip_path.as_ptr().cast::<libc::c_char>();
        let bytes =
            unsafe { std::slice::from_raw_parts(path.cast::<u8>(), libc::MAXPATHLEN as usize) };
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    }

    fn c_chars<const N: usize>(value: &[libc::c_char; N]) -> String {
        let bytes = unsafe { std::slice::from_raw_parts(value.as_ptr().cast::<u8>(), N) };
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    }
}

#[cfg(target_os = "macos")]
fn unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(not(target_os = "macos"))]
pub(super) struct ProcessSampler {
    sample_poll: Timer,
    discovery_poll: Timer,
    system: sysinfo::System,
}

#[cfg(not(target_os = "macos"))]
impl Default for ProcessSampler {
    fn default() -> Self {
        Self {
            sample_poll: Timer::from_seconds(2.0, TimerMode::Repeating),
            discovery_poll: Timer::from_seconds(300.0, TimerMode::Repeating),
            system: sysinfo::System::new(),
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl ProcessSampler {
    pub fn refresh(
        &mut self,
        delta: std::time::Duration,
        immediate: bool,
        service_roots: impl IntoIterator<Item = u32>,
    ) -> Option<ProcessSnapshot> {
        self.sample_poll.tick(delta);
        self.discovery_poll.tick(delta);
        if !immediate && !self.sample_poll.just_finished() {
            return None;
        }
        if immediate || self.system.processes().is_empty() || self.discovery_poll.just_finished() {
            self.discovery_poll.reset();
            self.system.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::All,
                true,
                sysinfo::ProcessRefreshKind::nothing(),
            );
        }
        self.sample_poll.reset();
        let monitored = MonitoredProcesses::new(&self.system, service_roots);
        self.system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&monitored.pids),
            true,
            sysinfo::ProcessRefreshKind::nothing()
                .with_memory()
                .with_cpu()
                .with_exe(sysinfo::UpdateKind::OnlyIfNotSet)
                .with_cwd(sysinfo::UpdateKind::OnlyIfNotSet),
        );
        Some(monitored.snapshot(&self.system))
    }
}

#[cfg(not(target_os = "macos"))]
struct MonitoredProcesses {
    pids: Vec<sysinfo::Pid>,
    local: HashSet<u32>,
}

#[cfg(not(target_os = "macos"))]
impl MonitoredProcesses {
    fn new(system: &sysinfo::System, service_roots: impl IntoIterator<Item = u32>) -> Self {
        let mut roots = service_roots.into_iter().collect::<HashSet<_>>();
        let mut local = HashSet::new();
        let mut children = HashMap::<u32, Vec<u32>>::new();
        for (pid, process) in system.processes() {
            let pid = pid.as_u32();
            if let Some(parent) = process.parent() {
                children.entry(parent.as_u32()).or_default().push(pid);
            }
            let name = process.name().to_string_lossy();
            let executable = process
                .exe()
                .map(|path| path.to_string_lossy())
                .unwrap_or_default();
            if LocalProcess::matches(&name, &executable) {
                roots.insert(pid);
                local.insert(pid);
            }
        }
        let mut selected = roots.clone();
        let mut pending = roots.into_iter().collect::<Vec<_>>();
        while let Some(parent) = pending.pop() {
            let Some(descendants) = children.get(&parent) else {
                continue;
            };
            for child in descendants {
                if selected.insert(*child) {
                    pending.push(*child);
                }
            }
        }
        let mut pids = selected
            .into_iter()
            .map(sysinfo::Pid::from_u32)
            .collect::<Vec<_>>();
        pids.sort_by_key(|pid| pid.as_u32());
        Self { pids, local }
    }

    fn snapshot(&self, system: &sysinfo::System) -> ProcessSnapshot {
        let mut samples = HashMap::new();
        let mut local = Vec::new();
        for pid in &self.pids {
            let Some(process) = system.process(*pid) else {
                continue;
            };
            samples.insert(
                pid.as_u32(),
                ProcSample {
                    parent: process.parent().map(|parent| parent.as_u32()),
                    cpu: process.cpu_usage(),
                    mem: process.memory(),
                },
            );
            if self.local.contains(&pid.as_u32()) {
                let name = process.name().to_string_lossy().into_owned();
                let executable = process
                    .exe()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default();
                local.push((
                    pid.as_u32(),
                    LocalProcess {
                        shell: if executable.is_empty() {
                            name
                        } else {
                            executable
                        },
                        cwd: process
                            .cwd()
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        uptime_secs: process.run_time(),
                    },
                ));
            }
        }
        ProcessSnapshot { samples, local }
    }
}

impl LocalProcess {
    fn matches(name: &str, executable: &str) -> bool {
        if name.to_ascii_lowercase().contains("vmux") {
            return true;
        }
        let executable_name = executable.rsplit('/').next().unwrap_or(executable);
        executable_name.to_ascii_lowercase().contains("vmux")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vmux_process_match_uses_process_or_executable_name() {
        assert!(LocalProcess::matches("vmux_desktop Helper", ""));
        assert!(LocalProcess::matches(
            "helper",
            "/Applications/Vmux.app/Contents/MacOS/vmux_service"
        ));
        assert!(!LocalProcess::matches(
            "codex",
            "/Users/test/.vmux/projects/repo/codex"
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn process_counter_uses_cumulative_cpu_delta() {
        let mut counter = ProcessCounter::new(10, 100, 20);
        assert_eq!(counter.update(10, 200, 70), 50.0);
        assert_eq!(counter.update(11, 300, 90), 0.0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_sampler_reads_the_current_process() {
        let pid = std::process::id();
        assert!(MacProcess::all().contains(&pid));
        assert!(MacProcess::new(pid).unwrap().task().is_some());
    }
}
