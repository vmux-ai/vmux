use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::http::{Credential, CredentialSource};
use crate::native::Endpoint;
use crate::runtime::Now;
use vmux_ecs::{JobSet, Revision};

pub(crate) struct AdministrationAccessPlugin;

impl Plugin for AdministrationAccessPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, bind.after(crate::Set::Clock).before(JobSet::Apply));
    }
}

#[derive(SystemParam)]
pub(crate) struct Access<'w, 's> {
    sources: Query<'w, 's, &'static CredentialSource>,
    credentials: Query<'w, 's, (&'static Credential, &'static Revision)>,
    now: Res<'w, Now>,
}

impl Access<'_, '_> {
    pub(crate) fn credential(
        &self,
        provider: Entity,
        endpoint: &Endpoint,
    ) -> Result<Option<&Credential>, String> {
        let source = self.sources.get(provider).ok();
        match (source, endpoint.source) {
            (None, None) => Ok(None),
            (Some(source), Some((expected, revision))) if source.0 == expected => {
                let (credential, current) = self
                    .credentials
                    .get(expected)
                    .map_err(|_| "features-authorization-unavailable")?;
                if current.0 != revision
                    || credential.environment() != endpoint.environment
                    || credential.expires_at() <= self.now.0
                {
                    return Err("features-authorization-unavailable".into());
                }
                Ok(Some(credential))
            }
            _ => Err("features-authorization-unavailable".into()),
        }
    }

    pub(crate) fn capture(
        &self,
        provider: Entity,
        endpoint: &Endpoint,
    ) -> Result<Endpoint, String> {
        Ok(endpoint.capture(self.credential(provider, endpoint)?))
    }

    pub(crate) fn current(&self, provider: Entity, endpoint: &Endpoint) -> bool {
        self.credential(provider, endpoint).is_ok()
    }
}

fn bind(
    endpoints: Query<(Entity, &Endpoint, Option<&CredentialSource>)>,
    credentials: Query<(&Credential, &Revision)>,
    now: Res<Now>,
    mut commands: Commands,
) {
    for (provider, endpoint, source) in &endpoints {
        let mut current = None;
        if let Some(source) = source
            && let Ok((credential, revision)) = credentials.get(source.0)
            && credential.environment() == endpoint.environment
            && credential.expires_at() > now.0
        {
            current = Some((source.0, revision.0));
        }
        if endpoint.source == current {
            continue;
        }
        let mut next = if current.is_some() {
            endpoint
                .clone()
                .schema(2)
                .expect("supported administration schema")
        } else {
            endpoint.clone()
        };
        next.source = current;
        commands.entity(provider).insert(next);
    }
}
