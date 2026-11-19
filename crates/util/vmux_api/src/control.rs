use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;
use zeroize::Zeroize;

#[derive(Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ControlToken(String);

impl ControlToken {
    pub fn new(value: String) -> Result<Self, String> {
        let token = Self(value);
        if !token.valid() {
            return Err("invalid Control token".into());
        }
        Ok(token)
    }

    pub fn valid(&self) -> bool {
        if self.0.is_empty() || self.0.len() > 4096 {
            return false;
        }
        for byte in self.0.bytes() {
            if !byte.is_ascii_graphic() {
                return false;
            }
        }
        true
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ControlToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("[redacted]")
    }
}

impl Drop for ControlToken {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Serialize for ControlToken {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ControlToken {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ControlLease {
    pub token: ControlToken,
    pub account_id: Uuid,
    pub device_id: Uuid,
    pub workspace_id: Uuid,
    pub environment: String,
    pub expires_at: u64,
}

impl ControlLease {
    pub fn valid(&self) -> bool {
        self.token.valid()
            && !self.account_id.is_nil()
            && !self.device_id.is_nil()
            && !self.workspace_id.is_nil()
            && !self.environment.is_empty()
            && self.environment.len() <= 64
            && self.expires_at != 0
    }
}

#[derive(
    Clone, Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ControlHello {
    pub operation_id: Uuid,
    pub version: u16,
    pub installation: Uuid,
    pub environment: String,
}

#[derive(
    Clone, Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ControlReady {
    pub operation_id: Uuid,
    pub version: u16,
    pub nonce: Option<Uuid>,
    pub refusal: Option<CredentialOutcome>,
}

#[derive(
    Clone, Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct CredentialOffer {
    pub operation_id: Uuid,
    pub version: u16,
    pub nonce: Uuid,
    pub installation: Uuid,
    pub source_generation: u64,
    pub source_revision: u64,
    pub lease: ControlLease,
}

#[derive(
    Clone, Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct CredentialWithdraw {
    pub operation_id: Uuid,
    pub version: u16,
    pub nonce: Uuid,
    pub installation: Uuid,
    pub environment: String,
    pub source_generation: u64,
    pub source_revision: u64,
}

#[derive(
    Clone, Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct CredentialReply {
    pub operation_id: Uuid,
    pub nonce: Uuid,
    pub source_generation: u64,
    pub source_revision: u64,
    pub revision: u64,
    pub outcome: CredentialOutcome,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum CredentialOutcome {
    Applied,
    Refused,
    Unsupported,
    Invalid,
    Expired,
    Stale,
    Unavailable,
}

#[cfg(feature = "bevy")]
#[derive(bevy_ecs::schedule::SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ControlCredentialSet;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_lease_json_and_archive_preserve_the_wire_contract_without_debugging_the_token() {
        let secret = "fixture-derived-control-token";
        let wire = serde_json::json!({
            "token": secret,
            "account_id": "10000000-0000-0000-0000-000000000001",
            "device_id": "10000000-0000-0000-0000-000000000002",
            "workspace_id": "10000000-0000-0000-0000-000000000003",
            "environment": "development",
            "expires_at": 1795000000u64
        });
        let lease: ControlLease = serde_json::from_value(wire.clone()).unwrap();
        assert!(lease.valid());
        assert!(!format!("{lease:?}").contains(secret));
        let mut bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&lease).unwrap();
        let decoded = rkyv::from_bytes::<ControlLease, rkyv::rancor::Error>(&bytes).unwrap();
        bytes.as_mut_slice().zeroize();
        assert_eq!(serde_json::to_value(&decoded).unwrap(), wire);
        assert!(!format!("{decoded:?}").contains(secret));
    }

    #[test]
    fn native_token_decoding_refuses_whitespace_controls_unicode_and_oversized_values() {
        for invalid in [
            String::new(),
            "two words".into(),
            "line\nbreak".into(),
            "zero\0byte".into(),
            "非ascii".into(),
            "a".repeat(4097),
        ] {
            let wire = serde_json::to_string(&invalid).unwrap();
            assert!(serde_json::from_str::<ControlToken>(&wire).is_err());
        }
    }
}
