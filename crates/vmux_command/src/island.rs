pub const ISLAND_RENDER_EVENT: &str = "island-render";

#[derive(
    Clone,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct IslandRenderEvent {
    pub seq: u64,
    pub state: IslandState,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum IslandState {
    Idle,
    Search,
    Activity(IslandActivity),
    Notify(IslandNotice),
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct IslandActivity {
    pub kind: IslandActivityKind,
    pub label: String,
    pub progress: Option<f32>,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum IslandActivityKind {
    Agent,
    Terminal,
    Media,
    Download,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct IslandNotice {
    pub label: String,
    pub ttl_ms: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_render_event_roundtrips() {
        let ev = IslandRenderEvent {
            seq: 7,
            state: IslandState::Activity(IslandActivity {
                kind: IslandActivityKind::Agent,
                label: "vibe · editing".into(),
                progress: Some(0.62),
            }),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&ev).expect("ser");
        let back = rkyv::from_bytes::<IslandRenderEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(back, ev);
        assert_eq!(ISLAND_RENDER_EVENT, "island-render");
    }
}
