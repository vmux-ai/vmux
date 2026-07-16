#[cfg(all(feature = "bevy", not(target_arch = "wasm32")))]
use bevy_ecs::component::Component;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
#[cfg_attr(all(feature = "bevy", not(target_arch = "wasm32")), derive(Component))]
pub struct ProcessId(pub [u8; 16]);

impl Default for ProcessId {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessId {
    pub fn new() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }

    pub fn to_uuid(&self) -> uuid::Uuid {
        uuid::Uuid::from_bytes(self.0)
    }

    pub fn from_uuid(u: uuid::Uuid) -> Self {
        Self(*u.as_bytes())
    }
}

impl std::fmt::Display for ProcessId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_uuid())
    }
}

impl std::str::FromStr for ProcessId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_uuid(uuid::Uuid::parse_str(s)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_id_rkyv_roundtrip() {
        let original = ProcessId::new();
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<ProcessId, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original, recovered);
    }

    #[test]
    fn process_id_display_and_parse_roundtrip() {
        let original = ProcessId::new();
        let s = original.to_string();
        let parsed: ProcessId = s.parse().expect("parse");
        assert_eq!(original, parsed);
    }
}
