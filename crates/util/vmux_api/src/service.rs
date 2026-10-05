pub const RUN_OSC: &str = "6973";

#[cfg(feature = "bevy")]
pub trait ServiceMessageVariant: bevy_ecs::message::Message + Sized {
    fn from_service_message(message: &crate::protocol::ServiceMessage) -> Option<Self>;
}

#[vmux_api::ui_state(Default)]
pub struct ProcessesUiState {
    pub connected: bool,
    pub query: String,
    pub total_count: u32,
    pub managed_count: u32,
    pub cpu: ProcessUsageUiState,
    pub memory: ProcessUsageUiState,
    pub processes: Vec<ProcessEntry>,
}

#[vmux_api::contract(Default)]
pub struct ProcessUsageUiState {
    pub label: String,
    pub value: String,
    pub peak: String,
    pub line: String,
    pub area: String,
}

#[vmux_api::contract(Default)]
pub struct ProcessEntry {
    pub id: String,
    pub managed: bool,
    pub shell: String,
    pub shell_label: String,
    pub cwd: String,
    pub cwd_label: Option<String>,
    pub cols: u16,
    pub rows: u16,
    pub pid: u32,
    pub uptime_secs: u64,
    pub uptime_label: String,
    pub cpu_percent: f32,
    pub cpu_label: String,
    pub mem_bytes: u64,
    pub memory_label: String,
    pub attached: bool,
    pub preview_lines: Vec<PreviewLine>,
}

#[vmux_api::contract]
pub struct PreviewLine {
    pub text: String,
}

#[vmux_api::ui_event]
pub struct ProcessNavigateEvent {
    pub process_id: String,
    pub navigate: bool,
}

#[vmux_api::ui_event]
pub struct ProcessKillEvent {
    pub process_id: String,
    pub kill: bool,
}

#[vmux_api::ui_event]
pub struct ProcessKillAllEvent {
    pub kill_all: bool,
}

#[vmux_api::ui_event]
pub struct ProcessSearchRequest {
    pub query: String,
}

#[vmux_api::ui_event]
pub struct RelaunchRequest;
