#[vmux_api::ui_state(Default)]
pub struct ContainerUiState {
    pub supported: bool,
    pub running: bool,
    pub ready: bool,
    pub busy: bool,
    pub phase: String,
    pub data_directory: String,
    pub shared_directory: String,
    pub cpus: u8,
    pub memory_mib: u32,
    pub socket: String,
    pub error: String,
    pub logs: String,
    pub inventory_ready: bool,
    pub inventory_error: String,
    pub containers: Vec<ContainerEntry>,
    pub log_id: String,
    pub container_logs: String,
    pub container_log_error: String,
    pub container_logs_ready: bool,
    pub configuration_revision: u64,
    pub starting: bool,
    pub stopping: bool,
    pub project_starting: bool,
}

#[vmux_api::ui_event]
pub struct ContainerStartRequest;

#[vmux_api::ui_event]
pub struct ContainerSettingsRequest {
    pub shared_directory: String,
    pub cpus: u8,
    pub memory_mib: u32,
}

#[vmux_api::ui_event]
pub struct ContainerStopRequest;

#[vmux_api::ui_event]
pub struct ContainerLogsRequest;

#[vmux_api::ui_event]
pub struct ContainerCommandRequest {
    pub ids: Vec<String>,
    pub operation: ContainerOperation,
}

#[vmux_api::ui_event]
pub struct ContainerInspectRequest {
    pub id: String,
    pub logs: bool,
}

#[vmux_api::ui_event]
pub struct ContainerProjectOpenRequest;

#[vmux_api::contract(Copy, Eq)]
pub enum ContainerOperation {
    Start,
    Stop,
    Restart,
    Remove,
}

#[vmux_api::contract(Default, Eq)]
pub struct ContainerEntry {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub project: String,
    pub service: String,
    pub ports: Vec<ContainerPort>,
}

impl ContainerEntry {
    pub fn running(&self) -> bool {
        matches!(self.state.as_str(), "running" | "restarting" | "paused")
    }

    pub fn matches(&self, search: &str) -> bool {
        let search = search.trim().to_ascii_lowercase();
        for value in [&self.name, &self.image, &self.project, &self.service] {
            if value.to_ascii_lowercase().contains(&search) {
                return true;
            }
        }
        false
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct ContainerPort {
    pub host: u16,
    pub guest: u16,
    pub protocol: String,
}
