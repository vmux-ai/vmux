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
}

#[vmux_api::ui_event]
pub struct ContainerStartRequest {
    pub data_directory: String,
    pub shared_directory: String,
    pub cpus: u8,
    pub memory_mib: u32,
}

#[vmux_api::ui_event]
pub struct ContainerStopRequest;

#[vmux_api::ui_event]
pub struct ContainerLogsRequest;
