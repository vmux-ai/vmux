use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_api::Id;
use vmux_connector::ProviderKey;
use zeroize::Zeroizing;

use crate::connection::{Connected, Connection, ConnectionState};

pub struct CredentialLease {
    token: Zeroizing<String>,
    active: Arc<AtomicBool>,
}

impl CredentialLease {
    pub fn token(&self) -> Result<&str, String> {
        if !self.is_active() {
            return Err("connection is disconnected".to_string());
        }
        Ok(&self.token)
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
}

impl std::fmt::Debug for CredentialLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CredentialLease([REDACTED])")
    }
}

struct Credential {
    token: Zeroizing<String>,
    active: Arc<AtomicBool>,
}

#[derive(Resource, Default)]
pub(crate) struct CredentialVault {
    credentials: HashMap<(ProviderKey, Id<Connection>), Credential>,
}

impl CredentialVault {
    pub(crate) fn install(
        &mut self,
        provider: ProviderKey,
        connection: Id<Connection>,
        token: impl Into<String>,
    ) {
        self.invalidate(&provider, &connection);
        self.credentials.insert(
            (provider, connection),
            Credential {
                token: Zeroizing::new(token.into()),
                active: Arc::new(AtomicBool::new(true)),
            },
        );
    }

    pub(crate) fn invalidate(&mut self, provider: &ProviderKey, connection: &Id<Connection>) {
        if let Some(credential) = self
            .credentials
            .remove(&(provider.clone(), connection.clone()))
        {
            credential.active.store(false, Ordering::Release);
        }
    }

    pub(crate) fn lease(
        &self,
        provider: &ProviderKey,
        connection: &Id<Connection>,
    ) -> Result<CredentialLease, String> {
        let credential = self
            .credentials
            .get(&(provider.clone(), connection.clone()))
            .ok_or_else(|| "connection credentials are unavailable".to_string())?;
        if !credential.active.load(Ordering::Acquire) {
            return Err("connection is disconnected".to_string());
        }
        Ok(CredentialLease {
            token: Zeroizing::new(credential.token.to_string()),
            active: Arc::clone(&credential.active),
        })
    }
}

#[derive(SystemParam)]
pub struct CredentialBroker<'w, 's> {
    vault: Res<'w, CredentialVault>,
    connections: Query<
        'w,
        's,
        (
            &'static Id<Connection>,
            &'static ProviderKey,
            &'static ConnectionState,
        ),
        With<Connection>,
    >,
}

impl CredentialBroker<'_, '_> {
    pub fn lease(
        &self,
        provider: &ProviderKey,
        connection: &Id<Connection>,
    ) -> Result<CredentialLease, String> {
        let (_, held_provider, state) = self
            .connections
            .iter()
            .find(|(held, _, _)| *held == connection)
            .ok_or_else(|| "connection is unavailable".to_string())?;
        if held_provider != provider {
            return Err("connection belongs to another auth provider".to_string());
        }
        if state != &ConnectionState::Connected {
            return Err("connection is not connected".to_string());
        }
        self.vault.lease(provider, connection)
    }
}

#[derive(SystemParam)]
pub struct CredentialIssuer<'w, 's> {
    vault: ResMut<'w, CredentialVault>,
    connections: Query<
        'w,
        's,
        (
            Entity,
            &'static Id<Connection>,
            &'static ProviderKey,
            &'static mut ConnectionState,
        ),
        With<Connection>,
    >,
    commands: Commands<'w, 's>,
}

impl CredentialIssuer<'_, '_> {
    pub fn issue(
        &mut self,
        provider: &ProviderKey,
        connection: &Id<Connection>,
        token: impl Into<String>,
    ) -> Result<Entity, String> {
        let Some((entity, id, held_provider, mut state)) = self
            .connections
            .iter_mut()
            .find(|(_, id, _, _)| *id == connection)
        else {
            return Err("connection is unavailable".to_string());
        };
        if held_provider != provider {
            return Err("connection belongs to another auth provider".to_string());
        }
        self.vault
            .install(provider.clone(), connection.clone(), token);
        *state = ConnectionState::Connected;
        self.commands.trigger(Connected {
            entity,
            id: id.clone(),
            provider: held_provider.clone(),
        });
        Ok(entity)
    }

    pub fn fail(
        &mut self,
        connection: &Id<Connection>,
        error: impl Into<String>,
    ) -> Result<Entity, String> {
        let Some((entity, _, _, mut state)) = self
            .connections
            .iter_mut()
            .find(|(_, id, _, _)| *id == connection)
        else {
            return Err("connection is unavailable".to_string());
        };
        *state = ConnectionState::Failed(error.into());
        Ok(entity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_redacts_tokens_and_invalidation_revokes_leases() {
        let provider = ProviderKey::new("github").unwrap();
        let connection = Id::<Connection>::new("connection-1");
        let mut vault = CredentialVault::default();
        vault.install(provider.clone(), connection.clone(), "secret");
        let lease = vault.lease(&provider, &connection).unwrap();

        assert!(!format!("{lease:?}").contains("secret"));
        assert_eq!(lease.token().unwrap(), "secret");

        vault.invalidate(&provider, &connection);

        assert!(!lease.is_active());
        assert!(lease.token().is_err());
    }
}
