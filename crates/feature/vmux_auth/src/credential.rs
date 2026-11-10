use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::Id;
use vmux_connector::ProviderKey;
use vmux_ecs::manifest::{AuthFlow as AuthFlowManifest, AuthProvider};
use vmux_profile::oauth::OauthCredentials;
use zeroize::{Zeroize, Zeroizing};

use crate::connection::{
    Connection, ConnectionState, ConnectionValidationPending, CredentialStore,
};
use crate::flow::AuthFlow;
use crate::provider::AuthProviderDefinition;

pub struct CredentialLease {
    token: Zeroizing<String>,
    access: vmux_profile::oauth::OauthCredentialAccess,
}

impl CredentialLease {
    pub fn token(&self) -> Result<&str, String> {
        if !self.access.is_active() {
            return Err("Connection is disconnected".to_string());
        }
        Ok(&self.token)
    }

    pub fn is_active(&self) -> bool {
        self.access.is_active()
    }
}

impl std::fmt::Debug for CredentialLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CredentialLease([REDACTED])")
    }
}

#[derive(Clone)]
pub struct CredentialSource {
    provider: ProviderKey,
    connection: Id<Connection>,
    definition: AuthProvider,
    credentials: CredentialStore,
    access: vmux_profile::oauth::OauthCredentialAccess,
}

impl CredentialSource {
    pub fn lease(&self) -> Result<CredentialLease, String> {
        if let Some(lease) = self.fresh_lease()? {
            return Ok(lease);
        }
        self.access.refresh(|| {
            if let Some(lease) = self.fresh_lease()? {
                return Ok(lease);
            }
            let (revision, credentials) = self.load()?;
            let refreshed = self.refresh(&credentials)?;
            let stored = self.access.write_if_revision(revision, || {
                self.credentials.storage.store(
                    &self.provider.0,
                    self.connection.as_str(),
                    &refreshed,
                )
            });
            match stored {
                Ok(Some(())) => {
                    let _ = self.access.cache(refreshed.clone());
                    return Ok(self.lease_token(&refreshed));
                }
                Ok(None) => {
                    self.revoke_replacement(&refreshed, None)?;
                }
                Err(error) => {
                    self.revoke_replacement(&refreshed, Some(error))?;
                }
            }
            if !self.access.is_active() {
                return Err("Connection is disconnected".to_string());
            }
            self.fresh_lease()?
                .ok_or_else(|| "Connection credentials changed during refresh".to_string())
        })
    }

    fn fresh_lease(&self) -> Result<Option<CredentialLease>, String> {
        self.access.read(|| {
            let credentials = if let Some(credentials) = self.access.cached()? {
                credentials
            } else {
                let credentials = self
                    .credentials
                    .storage
                    .load(&self.provider.0, self.connection.as_str())?
                    .ok_or_else(|| "Connection credentials are unavailable".to_string())?;
                self.access.cache(credentials.clone())?;
                credentials
            };
            if credentials.expires_soon() {
                return Ok(None);
            }
            Ok(Some(self.lease_token(&credentials)))
        })
    }

    fn refresh(&self, credentials: &OauthCredentials) -> Result<OauthCredentials, String> {
        let refresh_token = credentials
            .refresh_token
            .as_ref()
            .ok_or_else(|| "Connection credentials expired and cannot be refreshed".to_string())?;
        let (client_id, token_endpoint, configured_scopes, headers) = match &self.definition.flow {
            AuthFlowManifest::DeviceCode(flow) => (
                &flow.client_id,
                &flow.token_endpoint,
                &flow.scopes,
                &flow.headers,
            ),
            AuthFlowManifest::AuthorizationCodePkce(flow) => (
                &flow.client_id,
                &flow.token_endpoint,
                &flow.scopes,
                &flow.headers,
            ),
        };
        let token_endpoint = AuthFlow::https_url(token_endpoint, "token endpoint")?;
        let client = AuthFlow::client()?;
        let response = AuthFlow::request(
            client.post(token_endpoint.clone()).form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.as_str()),
                ("client_id", client_id.as_str()),
            ]),
            headers,
        )?
        .send()
        .map_err(|error| error.to_string())?;
        let mut token: RefreshResponse = AuthFlow::json(response, "refresh")?;
        if token.access_token.trim().is_empty() {
            return Err("OAuth refresh response contains an empty access token".to_string());
        }
        Ok(OauthCredentials {
            client_id: client_id.to_string(),
            token_endpoint: token_endpoint.to_string(),
            access_token: std::mem::take(&mut token.access_token),
            refresh_token: token
                .refresh_token
                .take()
                .or_else(|| credentials.refresh_token.clone()),
            expires_at: OauthCredentials::expires_at(token.expires_in),
            scopes: token
                .scope
                .take()
                .map(|scope| scope.split_whitespace().map(str::to_string).collect())
                .unwrap_or_else(|| configured_scopes.to_vec()),
            token_type: token
                .token_type
                .take()
                .unwrap_or_else(|| credentials.token_type.clone()),
        })
    }

    fn load(&self) -> Result<(u64, OauthCredentials), String> {
        let (revision, credentials) = self.access.read_versioned(|| {
            if let Some(credentials) = self.access.cached()? {
                return Ok(Some(credentials));
            }
            let credentials = self
                .credentials
                .storage
                .load(&self.provider.0, self.connection.as_str())?;
            if let Some(credentials) = &credentials {
                self.access.cache(credentials.clone())?;
            }
            Ok(credentials)
        })?;
        let credentials =
            credentials.ok_or_else(|| "Connection credentials are unavailable".to_string())?;
        Ok((revision, credentials))
    }

    fn lease_token(&self, credentials: &OauthCredentials) -> CredentialLease {
        CredentialLease {
            token: Zeroizing::new(credentials.access_token.clone()),
            access: self.access.clone(),
        }
    }

    fn revoke_replacement(
        &self,
        credentials: &OauthCredentials,
        failure: Option<String>,
    ) -> Result<(), String> {
        let revoked = AuthFlow::revoke_retrying(&self.definition, credentials, 3);
        match (failure, revoked) {
            (None, Ok(_)) => Ok(()),
            (Some(error), Ok(_)) => Err(error),
            (None, Err(error)) => Err(format!(
                "Replacement OAuth credentials could not be revoked: {error}"
            )),
            (Some(failure), Err(error)) => Err(format!(
                "{failure}; replacement OAuth credentials could not be revoked: {error}"
            )),
        }
    }
}

#[derive(SystemParam)]
pub struct CredentialBroker<'w, 's> {
    stores: Query<'w, 's, &'static CredentialStore>,
    providers: Query<'w, 's, (&'static ProviderKey, &'static AuthProviderDefinition)>,
    connections: BrokerConnections<'w, 's>,
}

type BrokerConnections<'w, 's> = Query<
    'w,
    's,
    (
        &'static Id<Connection>,
        &'static ProviderKey,
        &'static ConnectionState,
        Option<&'static ConnectionValidationPending>,
    ),
    With<Connection>,
>;

impl CredentialBroker<'_, '_> {
    pub fn source(
        &self,
        provider: &ProviderKey,
        connection: &Id<Connection>,
    ) -> Result<CredentialSource, String> {
        self.resolve(connection, Some(provider), false)
    }

    pub fn validation_source(
        &self,
        connection: &Id<Connection>,
    ) -> Result<CredentialSource, String> {
        self.resolve(connection, None, true)
    }

    fn resolve(
        &self,
        connection: &Id<Connection>,
        expected_provider: Option<&ProviderKey>,
        allow_validation: bool,
    ) -> Result<CredentialSource, String> {
        let (_, provider, state, validation) = self
            .connections
            .iter()
            .find(|(held, _, _, _)| *held == connection)
            .ok_or_else(|| "Connection is unavailable".to_string())?;
        if expected_provider.is_some_and(|expected| expected != provider) {
            return Err("Connection belongs to another auth provider".to_string());
        }
        if state != &ConnectionState::Connected {
            return Err("Connection is not connected".to_string());
        }
        if validation.is_some() && !allow_validation {
            return Err("Connection identity validation is pending".to_string());
        }
        let credentials = self
            .stores
            .single()
            .map_err(|_| "OAuth credential store is unavailable".to_string())?;
        let definition = self
            .providers
            .iter()
            .find_map(|(key, definition)| (key == provider).then(|| definition.0.clone()))
            .ok_or_else(|| format!("Unknown auth provider: {}", provider.0))?;
        let access = credentials.access(provider, connection);
        if !access.is_active() {
            return Err("Connection is disconnected".to_string());
        }
        Ok(CredentialSource {
            provider: provider.clone(),
            connection: connection.clone(),
            definition,
            credentials: credentials.clone(),
            access,
        })
    }
}

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
    token_type: Option<String>,
}

impl Drop for RefreshResponse {
    fn drop(&mut self) {
        self.access_token.zeroize();
        if let Some(refresh_token) = &mut self.refresh_token {
            refresh_token.zeroize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource)]
    struct RequestedConnection(Id<Connection>);

    #[derive(Resource, Default)]
    struct CapturedError(Option<String>);

    fn capture_provider_mismatch(
        request: Res<RequestedConnection>,
        broker: CredentialBroker,
        mut captured: ResMut<CapturedError>,
    ) {
        captured.0 = broker
            .source(&ProviderKey("github".into()), &request.0)
            .err();
    }

    #[test]
    fn existing_lease_stops_exposing_its_token_after_disconnect() {
        let access = vmux_profile::oauth::OauthCredentialAccess::default();
        let lease = CredentialLease {
            token: Zeroizing::new("secret".to_string()),
            access: access.clone(),
        };

        assert_eq!(lease.token().unwrap(), "secret");
        access.invalidate();
        assert!(lease.token().is_err());
    }

    #[test]
    fn credential_source_rejects_a_different_provider() {
        let mut app = App::new();
        let id = Id::<Connection>::new("connection");
        app.insert_resource(RequestedConnection(id.clone()))
            .init_resource::<CapturedError>()
            .add_systems(Update, capture_provider_mismatch);
        app.world_mut().spawn((
            Connection,
            id,
            ProviderKey("linear".into()),
            ConnectionState::Connected,
        ));

        app.update();

        assert_eq!(
            app.world().resource::<CapturedError>().0.as_deref(),
            Some("Connection belongs to another auth provider")
        );
    }
}
