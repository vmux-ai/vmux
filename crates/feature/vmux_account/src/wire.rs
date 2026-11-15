#[cfg(feature = "cloud")]
use serde::Deserialize;
use serde::Serialize;
#[cfg(feature = "cloud")]
use uuid::Uuid;

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
            },
        }
    }
}
