#[vmux_api::ui_state(Default)]
pub struct SimulatorUiState {
    pub port: u16,
    pub capability: String,
    pub version: String,
    pub device_name: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub frame_stride: u32,
}

#[vmux_api::ui_event(Default)]
pub struct SimulatorTouch {
    pub phase: SimulatorTouchPhase,
    pub x: f32,
    pub y: f32,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub enum SimulatorTouchPhase {
    #[default]
    Down,
    Move,
    Up,
    Cancel,
    Tap,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct SimulatorInputTextRequest {
    pub text: String,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct SimulatorInputKeyRequest {
    pub code: u16,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct SimulatorInputModifiedKeyRequest {
    pub code: u16,
    pub modifiers: SimulatorKeyModifiers,
}

#[vmux_api::ui_event(Copy, Eq)]
pub struct SimulatorInputHardwareButtonRequest {
    pub button: HardwareButton,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub struct SimulatorKeyModifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

#[vmux_api::contract(Copy, Eq)]
pub enum HardwareButton {
    Home,
    Lock,
    Siri,
}

#[vmux_api::contract(Copy, Eq)]
pub enum SimulatorClipboardOperation {
    Copy,
    Cut,
    Paste,
    SelectAll,
}

#[vmux_api::ui_event]
pub struct SimulatorClipboardCopyRequest;

#[vmux_api::ui_event]
pub struct SimulatorClipboardCutRequest;

#[vmux_api::ui_event]
pub struct SimulatorClipboardPasteRequest;

#[vmux_api::ui_event]
pub struct SimulatorClipboardSelectAllRequest;

#[vmux_api::ui_event]
pub struct SimulatorSoftwareKeyboard;

impl HardwareButton {
    pub fn as_arg(&self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Lock => "lock",
            Self::Siri => "siri",
        }
    }
}

impl SimulatorKeyModifiers {
    pub fn is_empty(self) -> bool {
        !self.control && !self.shift && !self.alt && !self.meta
    }

    pub fn hid_codes(self) -> Vec<u8> {
        let mut codes = Vec::with_capacity(4);
        if self.control {
            codes.push(224);
        }
        if self.shift {
            codes.push(225);
        }
        if self.alt {
            codes.push(226);
        }
        if self.meta {
            codes.push(227);
        }
        codes
    }
}
