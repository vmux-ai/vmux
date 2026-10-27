use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_connector::{
    ExternalId, ExternalLink, ExternalLinkIdentity, ExternalScopeRef, ProviderKey, ResourceKind,
};
use vmux_ecs::{RemoteUpdatedAt, UnixMillis};
use vmux_session::ProviderSession;

pub(crate) struct AcpSessionLink;

impl AcpSessionLink {
    pub(crate) fn kind(agent: &str) -> String {
        format!("acp:{agent}")
    }

    pub(crate) fn identity(kind: &str, id: &str) -> Option<ExternalLinkIdentity> {
        ProviderSession::external_identity("acp", kind, id)
    }
}

pub(crate) struct AcpSessionRef<'a> {
    kind: &'a str,
    id: &'a ExternalId,
}

impl AcpSessionRef<'_> {
    pub(crate) fn kind(&self) -> &str {
        self.kind
    }

    pub(crate) fn id(&self) -> &ExternalId {
        self.id
    }
}

#[derive(SystemParam)]
pub(crate) struct AcpSessionLinks<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    links: Query<
        'w,
        's,
        (
            Entity,
            &'static ProviderKey,
            &'static ExternalScopeRef,
            &'static ResourceKind,
            &'static ExternalId,
        ),
        With<ExternalLink>,
    >,
}

impl AcpSessionLinks<'_, '_> {
    fn spawn(
        &self,
        commands: &mut Commands,
        session: Entity,
        identity: &ExternalLinkIdentity,
        remote_updated_at: Option<i64>,
    ) -> Entity {
        let mut link = commands.spawn((
            ExternalLink,
            identity.provider().clone(),
            ExternalScopeRef::new(identity.scope().clone()),
            identity.kind().clone(),
            identity.external_id().clone(),
            ChildOf(session),
        ));
        if let Some(remote_updated_at) = remote_updated_at {
            link.insert(RemoteUpdatedAt(UnixMillis(remote_updated_at)));
        }
        link.id()
    }

    pub(crate) fn get(&self, session: Entity) -> Option<AcpSessionRef<'_>> {
        let children = self.children.get(session).ok()?;
        for child in children.iter() {
            let Ok((_, provider, scope, kind, id)) = self.links.get(child) else {
                continue;
            };
            if provider.as_str() == "acp" && kind.as_str() == "session" {
                return Some(AcpSessionRef {
                    kind: scope.id().as_str(),
                    id,
                });
            }
        }
        None
    }

    pub(crate) fn set(
        &self,
        commands: &mut Commands,
        session: Entity,
        kind: &str,
        id: &str,
        remote_updated_at: Option<i64>,
    ) -> Option<Entity> {
        let identity = AcpSessionLink::identity(kind, id)?;
        let mut current = None;
        if let Ok(children) = self.children.get(session) {
            for child in children.iter() {
                let Ok((entity, provider, scope, resource, external_id)) = self.links.get(child)
                else {
                    continue;
                };
                if provider.as_str() != "acp" || resource.as_str() != "session" {
                    continue;
                }
                if current.is_none()
                    && scope.id().as_str() == kind
                    && external_id == identity.external_id()
                {
                    current = Some(entity);
                } else {
                    commands.entity(entity).despawn();
                }
            }
        }
        if let Some(current) = current {
            if let Some(remote_updated_at) = remote_updated_at {
                commands
                    .entity(current)
                    .insert(RemoteUpdatedAt(UnixMillis(remote_updated_at)));
            }
            return Some(current);
        }
        Some(self.spawn(commands, session, &identity, remote_updated_at))
    }

    pub(crate) fn remove(&self, commands: &mut Commands, session: Entity) {
        let Ok(children) = self.children.get(session) else {
            return;
        };
        for child in children.iter() {
            let Ok((entity, provider, _, resource, _)) = self.links.get(child) else {
                continue;
            };
            if provider.as_str() == "acp" && resource.as_str() == "session" {
                commands.entity(entity).despawn();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    struct Fixture {
        world: World,
        session: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let mut world = World::new();
            let session = world.spawn_empty().id();
            Self { world, session }
        }

        fn set(&mut self, kind: &'static str, id: &'static str) {
            let session = self.session;
            self.world
                .run_system_once(move |links: AcpSessionLinks, mut commands: Commands| {
                    links.set(&mut commands, session, kind, id, None);
                })
                .unwrap();
            self.world.flush();
        }

        fn links(&mut self) -> Vec<(Entity, String, String)> {
            let mut query = self
                .world
                .query_filtered::<(Entity, &ExternalScopeRef, &ExternalId), With<ExternalLink>>();
            query
                .iter(&self.world)
                .map(|(entity, scope, id)| {
                    (
                        entity,
                        scope.id().as_str().to_string(),
                        id.as_str().to_string(),
                    )
                })
                .collect()
        }
    }

    #[test]
    fn replacing_a_provider_session_link_leaves_one_owned_link() {
        let mut fixture = Fixture::new();
        fixture.set("acp:codex", "old");
        fixture.set("acp:codex", "new");

        let links = fixture.links();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].1, "acp:codex");
        assert_eq!(links[0].2, "new");
        assert_eq!(
            fixture.world.get::<ChildOf>(links[0].0).unwrap().parent(),
            fixture.session
        );
    }
}
