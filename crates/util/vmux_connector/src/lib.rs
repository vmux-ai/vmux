use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::HashSet;
use vmux_ecs::store::StoreAppExt;

pub use external::*;
mod external;

pub struct ConnectorPlugin;

impl Plugin for ConnectorPlugin {
    fn build(&self, app: &mut App) {
        app.register_stored::<ExternalLink>()
            .register_stored::<ExternalScope>()
            .register_stored::<vmux_api::Id<ExternalScope>>()
            .register_stored::<ProviderKey>()
            .register_stored::<ExternalHost>()
            .register_stored::<ExternalNamespace>()
            .register_stored::<ExternalScopeRef>()
            .register_type::<CanAccess>()
            .register_type::<AccountExternalId>()
            .register_stored::<ExternalId>()
            .register_stored::<ExternalKey>()
            .register_stored::<ExternalUrl>()
            .register_stored::<ResourceKind>()
            .register_stored::<SyncPolicy>()
            .register_stored::<SynchronizedFields>()
            .register_stored::<PrimaryWorkItem>()
            .register_stored::<ReviewArtifact>()
            .init_resource::<ExternalLinkIndex>()
            .init_resource::<IndexedExternalLinks>()
            .add_systems(PreUpdate, index_external_links);
    }
}

#[derive(Resource, Default)]
struct IndexedExternalLinks(HashSet<Entity>);

type ExternalLinks<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ProviderKey,
        &'static ExternalScopeRef,
        &'static ResourceKind,
        &'static ExternalId,
        &'static ChildOf,
        Option<&'static PrimaryWorkItem>,
        Option<&'static SyncPolicy>,
        Option<&'static SynchronizedFields>,
    ),
    With<ExternalLink>,
>;

fn index_external_links(
    links: ExternalLinks,
    mut changes: ExternalLinkIndexChanges,
    mut current: ResMut<ExternalLinkIndex>,
    mut indexed: ResMut<IndexedExternalLinks>,
) {
    if !changes.dirty(&indexed) {
        return;
    }
    let mut next = ExternalLinkIndex::default();
    let mut next_indexed = HashSet::new();
    for (entity, provider, scope, kind, id, parent, primary, policy, fields) in &links {
        next_indexed.insert(entity);
        next.include(ExternalLinkSpec {
            identity: ExternalLinkIdentity {
                provider: provider.clone(),
                scope: scope.0.clone(),
                kind: kind.clone(),
                id: id.clone(),
            },
            parent: parent.parent(),
            primary: primary.is_some(),
            sync_policy: policy.copied().unwrap_or_default(),
            fields: fields.map_or(&[], |fields| fields.0.as_slice()),
        });
    }
    if *current != next {
        *current = next;
    }
    indexed.0 = next_indexed;
}

type ChangedExternalLinks<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<ExternalLink>,
        Or<(
            Added<ExternalLink>,
            Changed<ProviderKey>,
            Changed<ExternalScopeRef>,
            Changed<ResourceKind>,
            Changed<ExternalId>,
            Changed<ChildOf>,
            Changed<PrimaryWorkItem>,
            Changed<SyncPolicy>,
            Changed<SynchronizedFields>,
        )>,
    ),
>;

#[derive(SystemParam)]
struct ExternalLinkIndexChanges<'w, 's> {
    changed: ChangedExternalLinks<'w, 's>,
    links: RemovedComponents<'w, 's, ExternalLink>,
    providers: RemovedComponents<'w, 's, ProviderKey>,
    scopes: RemovedComponents<'w, 's, ExternalScopeRef>,
    kinds: RemovedComponents<'w, 's, ResourceKind>,
    ids: RemovedComponents<'w, 's, ExternalId>,
    parents: RemovedComponents<'w, 's, ChildOf>,
    primary: RemovedComponents<'w, 's, PrimaryWorkItem>,
    policies: RemovedComponents<'w, 's, SyncPolicy>,
    fields: RemovedComponents<'w, 's, SynchronizedFields>,
}

impl ExternalLinkIndexChanges<'_, '_> {
    fn dirty(&mut self, indexed: &IndexedExternalLinks) -> bool {
        let mut dirty = !self.changed.is_empty();
        for entity in self.links.read() {
            dirty |= indexed.0.contains(&entity);
        }
        for entity in self.providers.read() {
            dirty |= indexed.0.contains(&entity);
        }
        for entity in self.scopes.read() {
            dirty |= indexed.0.contains(&entity);
        }
        for entity in self.kinds.read() {
            dirty |= indexed.0.contains(&entity);
        }
        for entity in self.ids.read() {
            dirty |= indexed.0.contains(&entity);
        }
        for entity in self.parents.read() {
            dirty |= indexed.0.contains(&entity);
        }
        for entity in self.primary.read() {
            dirty |= indexed.0.contains(&entity);
        }
        for entity in self.policies.read() {
            dirty |= indexed.0.contains(&entity);
        }
        for entity in self.fields.read() {
            dirty |= indexed.0.contains(&entity);
        }
        dirty
    }
}
