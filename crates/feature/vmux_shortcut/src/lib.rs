#[cfg(host)]
pub use host::{ShortcutCaptureSet, ShortcutPlugin};

pub(crate) struct Feature;

impl vmux_feature::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(host)]
pub struct ShortcutUrl;

#[cfg(host)]
impl ShortcutUrl {
    pub fn canonical(task: &vmux_ecs::PageOpenTask) -> Option<&'static str> {
        let url = task.url.trim().trim_end_matches('/');
        (url == ShortcutPlugin::url().trim_end_matches('/')
            || matches!(url, "vmux://cheatsheet" | "vmux://cheetsheet"))
        .then_some(ShortcutPlugin::url())
    }
}

#[cfg(host)]
#[derive(bevy::prelude::EntityEvent, Clone, Debug)]
pub struct ShortcutProbePress {
    #[event_target]
    target: bevy::prelude::Entity,
    stroke: ShortcutStroke,
    pressed_at_ms: i64,
}

#[cfg(host)]
impl ShortcutProbePress {
    pub fn new(target: bevy::prelude::Entity, stroke: ShortcutStroke, pressed_at_ms: i64) -> Self {
        Self {
            target,
            stroke,
            pressed_at_ms,
        }
    }

    pub fn stroke(&self) -> &ShortcutStroke {
        &self.stroke
    }

    pub fn pressed_at_ms(&self) -> i64 {
        self.pressed_at_ms
    }
}

#[vmux_api::ui_event(Eq)]
pub struct ShortcutProbePressRequest {
    pub stroke: ShortcutStroke,
}

#[vmux_api::ui_event]
pub struct ShortcutProbeClearRequest;

#[vmux_api::ui_event(Copy, Eq)]
pub struct ShortcutCollectionViewRequest {
    pub view: vmux_ecs::CollectionView,
}

#[vmux_api::contract(Default, Eq)]
pub enum ShortcutProbeStatus {
    #[default]
    Idle,
    Pending,
    Match(Vec<String>),
    Contextual(Vec<String>),
    Miss,
}

#[vmux_api::ui_state(Default)]
pub struct ShortcutUiState {
    pub groups: Vec<ShortcutGroup>,
    pub probe_sequence: Vec<ShortcutStroke>,
    pub probe_status: ShortcutProbeStatus,
    pub shortcut_count: u32,
    pub collection_view: vmux_ecs::CollectionView,
}

#[vmux_api::contract(Default, Eq)]
pub struct ShortcutGroup {
    pub name: String,
    pub entries: Vec<ShortcutEntry>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ShortcutEntry {
    pub id: String,
    pub name: String,
    pub shortcuts: Vec<ShortcutBinding>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ShortcutBinding {
    pub label: String,
    pub strokes: Vec<ShortcutStroke>,
    pub resolves: bool,
    pub contexts: Vec<String>,
    #[serde(default)]
    pub emphasized: bool,
}

#[vmux_api::contract(Default, Eq, Hash)]
pub struct ShortcutStroke {
    pub code: String,
    pub label: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
}

#[cfg(host)]
mod claim;
#[cfg(host)]
mod host;
#[cfg(host)]
mod input;

#[cfg(ui)]
pub mod ui;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_state_round_trips_projected_shortcuts() {
        let state = ShortcutUiState {
            probe_sequence: vec![ShortcutStroke {
                code: "KeyK".to_string(),
                label: "K".to_string(),
                ctrl: true,
                ..Default::default()
            }],
            probe_status: ShortcutProbeStatus::Pending,
            shortcut_count: 3,
            ..Default::default()
        };

        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&state).unwrap();
        let decoded = rkyv::from_bytes::<ShortcutUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(decoded.shortcut_count, 3);
        assert_eq!(decoded.probe_sequence[0].code, "KeyK");
        assert_eq!(decoded.probe_status, ShortcutProbeStatus::Pending);
    }
}
