use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use moonshine_save::prelude::*;
use std::collections::HashSet;
use vmux_app::VmuxEcsPlugin;
use vmux_ecs::store::SchemaPlugin;
use vmux_ecs::{
    Cwd, IdentityIndex, IdentityIndexPlugin, IdentityIndexSet, OperationCompleted, OperationState,
    Revision,
};

pub struct DocumentsPlugin;

impl Plugin for DocumentsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            VmuxEcsPlugin,
            IdentityIndexPlugin::<DocumentId, Document>::default(),
        ))
        .add_message::<RenameRequest>()
        .add_systems(
            Update,
            rename.after(IdentityIndexSet::of::<DocumentId, Document>()),
        );
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect)]
#[reflect(Component)]
pub struct DocumentId(pub String);

#[derive(Component, Default, Reflect)]
#[reflect(Component)]
pub struct Document;

#[derive(Message)]
pub struct RenameRequest {
    pub operation: Entity,
    pub owner: Entity,
    pub id: DocumentId,
    pub name: String,
}

fn rename(
    mut requests: MessageReader<RenameRequest>,
    identities: Res<IdentityIndex<DocumentId, Document>>,
    operations: Query<&OperationState>,
    mut documents: Query<(&mut Name, &mut Revision), With<Document>>,
    mut commands: Commands,
) {
    let mut consumed = HashSet::new();
    for request in requests.read() {
        if !matches!(
            operations.get(request.operation),
            Ok(OperationState::Pending | OperationState::Running)
        ) {
            continue;
        }
        if !consumed.insert(request.operation) {
            continue;
        }
        let entity = match identities.entity_in(request.owner, &request.id) {
            Ok(Some(entity)) => entity,
            _ => {
                commands.trigger(OperationCompleted {
                    entity: request.operation,
                    result: Err("document identity is unavailable".into()),
                });
                continue;
            }
        };
        let Ok((mut name, mut revision)) = documents.get_mut(entity) else {
            commands.trigger(OperationCompleted {
                entity: request.operation,
                result: Err("document was removed".into()),
            });
            continue;
        };
        let Some(next) = revision.next() else {
            commands.trigger(OperationCompleted {
                entity: request.operation,
                result: Err("document revision is exhausted".into()),
            });
            continue;
        };
        *name = Name::new(request.name.clone());
        *revision = next;
        commands.trigger(OperationCompleted {
            entity: request.operation,
            result: Ok(()),
        });
    }
}

pub struct DocumentPersistencePlugin;

impl Plugin for DocumentPersistencePlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<Document, Save>()
            .add_plugins((
                bevy_app::TaskPoolPlugin::default(),
                bevy_asset::AssetPlugin::default(),
                SchemaPlugin::<DocumentStore>::default(),
            ))
            .add_observer(save_on::<SaveWorld<With<Document>>>)
            .add_observer(load_on::<LoadWorld>);
    }
}

#[vmux_ecs::store(Document, DocumentId, Name, Cwd, Revision)]
struct DocumentStore;

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_ecs::store::Types;
    use vmux_ecs::{IdentityLookupError, Operation};

    #[test]
    fn independent_roots_share_components_and_complete_operations_without_a_ui() {
        let mut app = App::new();
        app.add_plugins(DocumentsPlugin);
        let first_root = app
            .world_mut()
            .spawn(Name::new("First application root"))
            .id();
        let second_root = app
            .world_mut()
            .spawn(Name::new("Second application root"))
            .id();
        let id = DocumentId("shared".into());
        let first = app
            .world_mut()
            .spawn((
                Document,
                id.clone(),
                Name::new("First"),
                Cwd("/first".into()),
                Revision(0),
                ChildOf(first_root),
            ))
            .id();
        let second = app
            .world_mut()
            .spawn((
                Document,
                id.clone(),
                Name::new("Second"),
                Cwd("/second".into()),
                Revision(0),
                ChildOf(second_root),
            ))
            .id();
        let operation = app
            .world_mut()
            .spawn(Operation::bundle("document.rename").unwrap().running())
            .id();
        app.world_mut().write_message(RenameRequest {
            operation,
            owner: first_root,
            id: id.clone(),
            name: "Renamed".into(),
        });
        app.world_mut().write_message(RenameRequest {
            operation,
            owner: second_root,
            id: id.clone(),
            name: "Same-frame replay".into(),
        });
        app.update();
        let index = app
            .world()
            .resource::<IdentityIndex<DocumentId, Document>>();
        assert_eq!(index.entity_in(first_root, &id), Ok(Some(first)));
        assert_eq!(index.entity_in(second_root, &id), Ok(Some(second)));
        assert_eq!(index.entity(&id), Err(IdentityLookupError::Duplicate(id)));
        assert_eq!(app.world().get::<Name>(first).unwrap().as_str(), "Renamed");
        assert_eq!(app.world().get::<Revision>(first).unwrap().0, 1);
        assert_eq!(app.world().get::<Name>(second).unwrap().as_str(), "Second");
        assert_eq!(app.world().get::<Revision>(second).unwrap().0, 0);
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Succeeded)
        );
        app.world_mut().write_message(RenameRequest {
            operation,
            owner: first_root,
            id: DocumentId("shared".into()),
            name: "Late replay".into(),
        });
        app.update();
        assert_eq!(app.world().get::<Name>(first).unwrap().as_str(), "Renamed");
        assert_eq!(app.world().get::<Revision>(first).unwrap().0, 1);
    }

    #[test]
    fn persistence_is_selected_by_the_feature_instead_of_the_shared_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("documents.ron");
        let mut app = App::new();
        app.add_plugins((DocumentsPlugin, DocumentPersistencePlugin));
        let transient = app
            .world_mut()
            .spawn((
                Name::new("Transient"),
                Cwd("/temporary".into()),
                Revision(3),
            ))
            .id();
        assert!(app.world().get::<Save>(transient).is_none());
        app.world_mut().spawn((
            Document,
            DocumentId("saved".into()),
            Name::new("Saved"),
            Cwd("/saved".into()),
            Revision(4),
        ));
        let mut save = SaveWorld::<With<Document>>::into_file(path.clone());
        app.world()
            .resource::<Types<DocumentStore>>()
            .apply(&mut save);
        app.world_mut().trigger_save(save);
        app.update();
        let mut restored = App::new();
        restored.add_plugins((DocumentsPlugin, DocumentPersistencePlugin));
        restored
            .world_mut()
            .trigger_load(LoadWorld::default_from_file(path));
        restored.update();
        let mut documents = restored
            .world_mut()
            .query::<(&DocumentId, &Name, &Cwd, &Revision)>();
        let rows = documents.iter(restored.world()).collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0.0, "saved");
        assert_eq!(rows[0].1.as_str(), "Saved");
        assert_eq!(rows[0].2.0, std::path::PathBuf::from("/saved"));
        assert_eq!(rows[0].3.0, 4);
    }
}
