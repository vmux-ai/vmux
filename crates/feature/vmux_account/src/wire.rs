#[cfg(feature = "cloud")]
use serde::Deserialize;
use serde::Serialize;
#[cfg(feature = "cloud")]
use uuid::Uuid;
#[cfg(feature = "cloud")]
use vmux_ecs::record::{Receipt, Record};

#[derive(Clone, Serialize)]
pub(crate) enum Request {
    AuthorizeDevice,
    Token {
        device_code: String,
    },
    RegisterDevice {
        session_token: String,
        device_id: String,
        kind: String,
        name: String,
        public_key: String,
    },
    Challenge {
        device_id: String,
    },
    Authenticate {
        signature: String,
    },
    Account,
    #[cfg(feature = "cloud")]
    Assertion,
    #[cfg(feature = "cloud")]
    ControlAccess {
        environment: String,
    },
    #[cfg(feature = "cloud")]
    Profiles {
        after: Option<Uuid>,
    },
    #[cfg(feature = "cloud")]
    ChangeProfile(ProfileChange),
}

#[cfg(feature = "cloud")]
#[derive(Clone, Serialize)]
pub(crate) struct ProfileChange {
    pub operation_id: Uuid,
    pub id: Uuid,
    pub base_revision: u64,
    pub name: Option<String>,
}

#[cfg(feature = "cloud")]
#[derive(Clone, Deserialize)]
pub(crate) struct WireAssertion {
    pub claims: String,
    pub signature: String,
}

#[cfg(feature = "cloud")]
impl WireAssertion {
    pub(crate) fn compact(&self) -> Option<String> {
        if self.claims.is_empty()
            || self.signature.is_empty()
            || !self.claims.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !self.signature.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        Some(format!("a1.{}.{}", self.claims, self.signature))
    }
}

#[cfg(feature = "cloud")]
#[derive(Deserialize)]
pub(crate) enum Response {
    DeviceAuthorization {
        device_code: String,
        user_code: String,
        verification_uri: String,
        expires_in_seconds: u64,
        interval_seconds: u64,
    },
    AuthorizationPending,
    SlowDown,
    ExpiredToken,
    AccessDenied,
    SignedIn {
        session_token: String,
    },
    DeviceRegistered,
    RegistrationRefused,
    Challenge {
        nonce: String,
    },
    Authenticated,
    NotAuthenticated,
    Account {
        id: Uuid,
    },
    Asserted {
        assertion: WireAssertion,
        refresh_in_seconds: u64,
    },
    ControlAccess {
        token: vmux_api::control::ControlToken,
        account_id: Uuid,
        device_id: Uuid,
        workspace_id: Uuid,
        environment: String,
        expires_at: u64,
    },
    ControlRefused,
    ControlUnavailable,
    Entities {
        entities: Vec<Record>,
        next: Option<Uuid>,
    },
    Committed(Receipt),
    Conflict {
        current: Option<Record>,
    },
    ProfileRefused,
    Busy,
    Failed,
    Malformed,
}

#[cfg(feature = "cloud")]
impl Request {
    pub(crate) fn accepts(&self, response: &Response) -> bool {
        match response {
            Response::Busy
            | Response::Failed
            | Response::Malformed
            | Response::NotAuthenticated => true,
            _ => match self {
                Self::AuthorizeDevice => matches!(response, Response::DeviceAuthorization { .. }),
                Self::Token { .. } => matches!(
                    response,
                    Response::AuthorizationPending
                        | Response::SlowDown
                        | Response::ExpiredToken
                        | Response::AccessDenied
                        | Response::SignedIn { .. }
                ),
                Self::RegisterDevice { .. } => matches!(
                    response,
                    Response::DeviceRegistered | Response::RegistrationRefused
                ),
                Self::Challenge { .. } => matches!(response, Response::Challenge { .. }),
                Self::Authenticate { .. } => matches!(response, Response::Authenticated),
                Self::Account => matches!(response, Response::Account { .. }),
                Self::Assertion => matches!(response, Response::Asserted { .. }),
                Self::ControlAccess { .. } => matches!(
                    response,
                    Response::ControlAccess { .. }
                        | Response::ControlRefused
                        | Response::ControlUnavailable
                ),
                Self::Profiles { .. } => matches!(response, Response::Entities { .. }),
                Self::ChangeProfile(_) => matches!(
                    response,
                    Response::Committed(_) | Response::Conflict { .. } | Response::ProfileRefused
                ),
            },
        }
    }
}
