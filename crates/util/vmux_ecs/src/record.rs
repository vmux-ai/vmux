use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::relationship::{Relationship, RelationshipTarget};
use bevy_ecs::system::{SystemParam, SystemState};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

pub use crate::{Revision, Uuid as Id};

pub struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Schema>()
            .configure_sets(
                Update,
                (Set::Restore, Set::Detect, Set::Capture, Set::Confirm).chain(),
            )
            .add_systems(Update, restore.in_set(Set::Restore))
            .add_systems(Update, detect.in_set(Set::Detect))
            .add_systems(Update, capture.in_set(Set::Capture))
            .add_systems(Update, confirm.in_set(Set::Confirm));
    }
}

#[derive(Component)]
pub struct Deleted;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredComponent {
    pub key: String,
    pub version: i32,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredRelationship {
    pub key: String,
    pub version: i32,
    pub target: Uuid,
    pub position: i32,
    pub owned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub operation_id: Uuid,
    pub entity_id: Uuid,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub scope_id: Uuid,
    pub id: Uuid,
    pub revision: u64,
    pub deleted: bool,
    pub components: Vec<StoredComponent>,
    pub relationships: Vec<StoredRelationship>,
}

impl Record {
    pub fn is_valid(&self) -> bool {
        if self.scope_id.is_nil()
            || self.id.is_nil()
            || self.revision == 0
            || self.revision > i64::MAX as u64
        {
            return false;
        }
        let mut components = HashSet::new();
        for component in &self.components {
            if component.key.is_empty()
                || component.version <= 0
                || !components.insert(&component.key)
            {
                return false;
            }
        }
        let mut relationships = HashSet::new();
        for relationship in &self.relationships {
            if relationship.key.is_empty()
                || relationship.version <= 0
                || relationship.position < 0
                || relationship.target.is_nil()
                || !relationships.insert((&relationship.key, relationship.position))
            {
                return false;
            }
        }
        true
    }
}

#[derive(Clone)]
struct Codec {
    key: &'static str,
    version: i32,
    encode: fn(&EntityRef<'_>) -> Result<Option<Value>, String>,
    decode: fn(&mut EntityWorldMut<'_>, Value) -> Result<(), String>,
}

impl Codec {
    fn encode<T: Component + Serialize>(entity: &EntityRef<'_>) -> Result<Option<Value>, String> {
        let Some(component) = entity.get::<T>() else {
            return Ok(None);
        };
        serde_json::to_value(component)
            .map(Some)
            .map_err(|error| error.to_string())
    }

    fn decode<T: Component + DeserializeOwned>(
        entity: &mut EntityWorldMut<'_>,
        value: Value,
    ) -> Result<(), String> {
        let component = serde_json::from_value::<T>(value).map_err(|error| error.to_string())?;
        entity.insert(component);
        Ok(())
    }
}

#[derive(Clone)]
struct RelationshipCodec {
    key: &'static str,
    version: i32,
    owned: bool,
    encode: fn(&World, Entity) -> Result<Option<Uuid>, String>,
    decode: fn(&mut World, Entity, Entity) -> Result<(), String>,
}

#[derive(Resource, Default, Clone)]
pub struct Schema {
    codecs: Vec<Codec>,
    relationships: Vec<RelationshipCodec>,
}

impl Schema {
    pub fn new() -> Self {
        Self {
            codecs: Vec::new(),
            relationships: Vec::new(),
        }
    }

    pub fn register<T: Component + Serialize + DeserializeOwned>(
        &mut self,
        key: &'static str,
        version: i32,
    ) {
        assert!(
            version > 0 && !key.is_empty() && !self.codecs.iter().any(|codec| codec.key == key)
        );
        self.codecs.push(Codec {
            key,
            version,
            encode: Codec::encode::<T>,
            decode: Codec::decode::<T>,
        });
    }

    pub fn register_relationship<R: Relationship>(&mut self, key: &'static str, version: i32) {
        assert!(
            version > 0
                && !key.is_empty()
                && !self.relationships.iter().any(|codec| codec.key == key)
        );
        self.relationships.push(RelationshipCodec {
            key,
            version,
            owned: <R::RelationshipTarget as RelationshipTarget>::LINKED_SPAWN,
            encode: CaptureInput::target::<R>,
            decode: RestoreInput::relate::<R>,
        });
    }

    pub fn capture(&self, entity: &EntityRef<'_>) -> Result<Vec<StoredComponent>, String> {
        let mut components = Vec::new();
        for codec in &self.codecs {
            if let Some(value) = (codec.encode)(entity)? {
                components.push(StoredComponent {
                    key: codec.key.to_string(),
                    version: codec.version,
                    value,
                });
            }
        }
        Ok(components)
    }

    pub fn restore(
        &self,
        entity: &mut EntityWorldMut<'_>,
        components: Vec<StoredComponent>,
    ) -> Result<(), String> {
        let mut seen = HashSet::new();
        for component in components {
            if !seen.insert(component.key.clone()) {
                return Err(format!("duplicate component {}", component.key));
            }
            let Some(codec) = self.codecs.iter().find(|codec| codec.key == component.key) else {
                return Err(format!("unknown component {}", component.key));
            };
            if codec.version != component.version {
                return Err(format!("unsupported version of {}", component.key));
            }
            (codec.decode)(entity, component.value)?;
        }
        Ok(())
    }
}

pub struct Graph(pub Vec<Record>);

impl Graph {
    fn validate(&self) -> Result<(), String> {
        let mut records = HashMap::new();
        let scope = self.0.first().map(|record| record.scope_id);
        for record in &self.0 {
            if !record.is_valid()
                || Some(record.scope_id) != scope
                || records.insert(record.id, record).is_some()
            {
                return Err("invalid graph scope, identity, or revision".to_string());
            }
        }
        let mut parents = HashMap::<Uuid, usize>::new();
        let mut children = HashMap::<Uuid, Vec<Uuid>>::new();
        for record in &self.0 {
            parents.insert(record.id, 0);
        }
        for record in &self.0 {
            for relationship in &record.relationships {
                let target = records
                    .get(&relationship.target)
                    .ok_or("unresolved relationship target")?;
                if relationship.owned {
                    if target.deleted && !record.deleted {
                        return Err("live entity has a deleted owner".to_string());
                    }
                    *parents.get_mut(&record.id).unwrap() += 1;
                    children.entry(target.id).or_default().push(record.id);
                }
            }
        }
        let mut ready = VecDeque::new();
        for (id, count) in &parents {
            if *count == 0 {
                ready.push_back(*id);
            }
        }
        let mut visited = 0;
        while let Some(id) = ready.pop_front() {
            visited += 1;
            if let Some(children) = children.get(&id) {
                for child in children {
                    let count = parents.get_mut(child).unwrap();
                    *count -= 1;
                    if *count == 0 {
                        ready.push_back(*child);
                    }
                }
            }
        }
        if visited != self.0.len() {
            return Err("ownership cycle".to_string());
        }
        Ok(())
    }
}

#[derive(Component)]
pub struct Scope(pub Uuid);

#[derive(Component)]
pub struct Persisted(pub u64);

#[derive(Component)]
pub struct Dirty;

#[derive(Component)]
#[relationship(relationship_target = StoreRoots)]
pub struct StoreOwner(pub Entity);

#[derive(Component, Default)]
#[relationship_target(relationship = StoreOwner, linked_spawn)]
pub struct StoreRoots(Vec<Entity>);

#[derive(Component)]
pub struct RestoreRequest(pub Graph);

#[derive(Component)]
pub struct Restored(pub Result<Vec<Entity>, String>);

#[derive(Component)]
pub struct CaptureRequest {
    pub scope_id: Uuid,
    pub entities: Vec<Entity>,
}

#[derive(Component)]
pub struct Captured(pub Result<Vec<Record>, String>);

#[derive(Component)]
pub struct Confirmed(pub Vec<(Entity, Receipt)>);

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Set {
    Restore,
    Detect,
    Capture,
    Confirm,
}

#[derive(SystemParam)]
struct RestoreInput<'w, 's> {
    requests: Query<'w, 's, Entity, With<RestoreRequest>>,
    schema: Res<'w, Schema>,
}

impl RestoreInput<'_, '_> {
    fn materialize(
        graph: Graph,
        world: &mut World,
        schema: &Schema,
    ) -> Result<Vec<Entity>, String> {
        graph.validate()?;
        let mut identities = HashMap::new();
        let mut entities = Vec::new();
        for record in &graph.0 {
            let mut entity = world.spawn((
                Id(record.id.to_string()),
                Revision(record.revision),
                Scope(record.scope_id),
            ));
            if record.deleted {
                entity.insert(Deleted);
            }
            let id = entity.id();
            identities.insert(record.id, id);
            entities.push(id);
        }
        let result = Self::apply(graph, world, schema, &identities);
        if let Err(error) = result {
            for entity in &entities {
                if let Ok(entity) = world.get_entity_mut(*entity) {
                    entity.despawn();
                }
            }
            return Err(error);
        }
        Ok(entities)
    }

    fn apply(
        graph: Graph,
        world: &mut World,
        schema: &Schema,
        identities: &HashMap<Uuid, Entity>,
    ) -> Result<(), String> {
        for record in &graph.0 {
            schema.restore(
                &mut world.entity_mut(identities[&record.id]),
                record.components.clone(),
            )?;
        }
        for record in graph.0 {
            for relationship in record.relationships {
                let Some(codec) = schema
                    .relationships
                    .iter()
                    .find(|codec| codec.key == relationship.key)
                else {
                    return Err(format!("unknown relationship {}", relationship.key));
                };
                if codec.version != relationship.version
                    || codec.owned != relationship.owned
                    || relationship.position != 0
                {
                    return Err(format!(
                        "unsupported relationship schema {}",
                        relationship.key
                    ));
                }
                (codec.decode)(
                    world,
                    identities[&record.id],
                    identities[&relationship.target],
                )?;
            }
        }
        Ok(())
    }

    fn relate<R: Relationship>(
        world: &mut World,
        source: Entity,
        target: Entity,
    ) -> Result<(), String> {
        if source == target && !R::ALLOW_SELF_REFERENTIAL {
            return Err("self relationship is not allowed".to_string());
        }
        world.entity_mut(source).insert(R::from(target));
        Ok(())
    }
}

fn restore(world: &mut World, pending: &mut SystemState<RestoreInput>) {
    let (requests, schema) = {
        let input = pending.get(world).expect("restore query");
        if input.requests.is_empty() {
            return;
        }
        (
            input.requests.iter().collect::<Vec<_>>(),
            input.schema.clone(),
        )
    };
    for request in requests {
        let graph = world
            .entity_mut(request)
            .take::<RestoreRequest>()
            .unwrap()
            .0;
        let mut metadata = Vec::new();
        for record in &graph.0 {
            metadata.push((
                record.scope_id,
                record.revision,
                !record.relationships.iter().any(|relation| relation.owned),
            ));
        }
        let restored = RestoreInput::materialize(graph, world, &schema);
        if let Ok(entities) = &restored {
            for (entity, (scope, revision, root)) in entities.iter().zip(metadata) {
                let mut entity = world.entity_mut(*entity);
                entity.insert((Scope(scope), Persisted(revision)));
                if root {
                    entity.insert(StoreOwner(request));
                }
            }
        }
        world.entity_mut(request).insert(Restored(restored));
    }
}

fn detect(
    changed: Query<(Entity, &Revision, &Persisted), Changed<Revision>>,
    mut commands: Commands,
) {
    for (entity, revision, persisted) in &changed {
        if revision.0 > persisted.0 {
            commands.entity(entity).insert(Dirty);
        }
    }
}

#[derive(SystemParam)]
struct CaptureInput<'w, 's> {
    requests: Query<'w, 's, Entity, With<CaptureRequest>>,
    schema: Res<'w, Schema>,
}

impl CaptureInput<'_, '_> {
    fn record(
        schema: &Schema,
        scope_id: Uuid,
        world: &World,
        entity: Entity,
    ) -> Result<Record, String> {
        let scope = world.get::<Scope>(entity).ok_or("missing entity scope")?;
        if scope.0 != scope_id {
            return Err("entity scope mismatch".to_string());
        }
        let id = world
            .get::<Id>(entity)
            .ok_or("missing stable entity identity")?;
        let revision = world
            .get::<Revision>(entity)
            .ok_or("missing entity revision")?;
        let mut relationships = Vec::new();
        for codec in &schema.relationships {
            if let Some(target) = (codec.encode)(world, entity)? {
                relationships.push(StoredRelationship {
                    key: codec.key.to_string(),
                    version: codec.version,
                    target,
                    position: 0,
                    owned: codec.owned,
                });
            }
        }
        relationships.sort_by(|left, right| {
            left.key
                .cmp(&right.key)
                .then(left.position.cmp(&right.position))
        });
        let mut components = schema.capture(&world.entity(entity))?;
        components.sort_by(|left, right| left.key.cmp(&right.key));
        let record = Record {
            scope_id,
            id: id.0.parse().map_err(|_| "invalid stable entity identity")?,
            revision: revision.0,
            deleted: world.get::<Deleted>(entity).is_some(),
            components,
            relationships,
        };
        if !record.is_valid() {
            return Err("invalid entity record".to_string());
        }
        Ok(record)
    }

    fn target<R: Relationship>(world: &World, entity: Entity) -> Result<Option<Uuid>, String> {
        let Some(relationship) = world.get::<R>(entity) else {
            return Ok(None);
        };
        let source_scope = world
            .get::<Scope>(entity)
            .ok_or("missing relationship source scope")?;
        let target_scope = world
            .get::<Scope>(relationship.get())
            .ok_or("missing relationship target scope")?;
        if source_scope.0 != target_scope.0 {
            return Err("cross-scope relationship".to_string());
        }
        let target = world
            .get::<Id>(relationship.get())
            .ok_or("relationship target has no stable identity")?;
        let id = target
            .0
            .parse()
            .map_err(|_| "invalid relationship target identity")?;
        Ok(Some(id))
    }
}

fn capture(world: &mut World, pending: &mut SystemState<CaptureInput>) {
    let (requests, schema) = {
        let input = pending.get(world).expect("capture query");
        if input.requests.is_empty() {
            return;
        }
        (
            input.requests.iter().collect::<Vec<_>>(),
            input.schema.clone(),
        )
    };
    for entity in requests {
        let request = world.entity_mut(entity).take::<CaptureRequest>().unwrap();
        let mut records = Vec::new();
        let mut result = Ok(());
        for entity in request.entities {
            match CaptureInput::record(&schema, request.scope_id, world, entity) {
                Ok(record) => records.push(record),
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        world
            .entity_mut(entity)
            .insert(Captured(result.map(|()| records)));
    }
}

fn confirm(
    mut requests: Query<(Entity, &mut Confirmed)>,
    mut revisions: Query<(&Id, &Revision, &mut Persisted)>,
    mut commands: Commands,
) {
    for (request, mut confirmed) in &mut requests {
        for (entity, receipt) in std::mem::take(&mut confirmed.0) {
            let Ok((id, revision, mut persisted)) = revisions.get_mut(entity) else {
                continue;
            };
            let Ok(id) = id.0.parse::<Uuid>() else {
                continue;
            };
            let committed = receipt.revision;
            if id != receipt.entity_id
                || receipt.operation_id.is_nil()
                || committed == 0
                || revision.0 < committed
            {
                continue;
            }
            persisted.0 = persisted.0.max(committed);
            if revision.0 == committed {
                commands.entity(entity).remove::<Dirty>();
            }
        }
        commands.entity(request).remove::<Confirmed>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Serialize, Deserialize)]
    struct Preferences {
        appearance: Appearance,
        fonts: Vec<Font>,
    }

    #[derive(Serialize, Deserialize)]
    enum Appearance {
        Custom {
            foreground: String,
            background: String,
        },
    }

    #[derive(Serialize, Deserialize)]
    struct Font {
        family: String,
        size: u32,
    }

    #[test]
    fn component_codecs_preserve_an_external_nested_schema_and_exclude_runtime_state() {
        let expected = serde_json::json!({
            "appearance": {"Custom": {"foreground": "#eeeeee", "background": "#111111"}},
            "fonts": [{"family": "Mono", "size": 14}, {"family": "Fallback", "size": 16}]
        });
        let mut schema = Schema::new();
        schema.register::<Preferences>("preferences", 2);
        let mut world = World::new();
        let entity = world.spawn(Dirty).id();
        schema
            .restore(
                &mut world.entity_mut(entity),
                vec![StoredComponent {
                    key: "preferences".to_string(),
                    version: 2,
                    value: expected.clone(),
                }],
            )
            .expect("restore independent fixture");
        let captured = schema
            .capture(&world.entity(entity))
            .expect("capture registered values");
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].key, "preferences");
        assert_eq!(captured[0].version, 2);
        assert_eq!(captured[0].value, expected);
        let restored = world.spawn_empty().id();
        schema
            .restore(&mut world.entity_mut(restored), captured)
            .expect("restore again");
        assert!(world.get::<Dirty>(restored).is_none());
        let captured = schema
            .capture(&world.entity(restored))
            .expect("capture restored values");
        assert_eq!(captured[0].value, expected);
    }

    impl Record {
        fn fixture(scope: Uuid, id: Uuid, name: &str, parent: Option<Uuid>) -> Self {
            Record {
                scope_id: scope,
                id,
                revision: 7,
                deleted: false,
                components: vec![StoredComponent {
                    key: "name".to_string(),
                    version: 1,
                    value: serde_json::json!(name),
                }],
                relationships: match parent {
                    Some(target) => vec![StoredRelationship {
                        key: "parent".to_string(),
                        version: 1,
                        target,
                        position: 0,
                        owned: true,
                    }],
                    None => Vec::new(),
                },
            }
        }
    }

    impl Schema {
        fn fixture() -> Self {
            let mut schema = Schema::new();
            schema.register::<Name>("name", 1);
            schema.register_relationship::<ChildOf>("parent", 1);
            schema
        }
    }

    #[test]
    fn store_systems_restore_child_of_capture_stable_ids_and_keep_newer_edits_dirty() {
        let scope = Uuid::new_v4();
        let root = Uuid::new_v4();
        let child = Uuid::new_v4();
        let mut app = App::new();
        app.add_plugins(Plugin).insert_resource(Schema::fixture());
        let request = app
            .world_mut()
            .spawn(RestoreRequest(Graph(vec![
                Record::fixture(scope, child, "Child", Some(root)),
                Record::fixture(scope, root, "Root", None),
            ])))
            .id();
        app.update();
        let Restored(Ok(entities)) = app.world().get::<Restored>(request).unwrap() else {
            panic!("expected restore")
        };
        let child_entity = entities[0];
        let root_entity = entities[1];
        assert_eq!(
            app.world().get::<ChildOf>(child_entity).unwrap().parent(),
            root_entity
        );
        assert!(app.world().get::<ChildOf>(root_entity).is_none());
        assert!(app.world().get::<Dirty>(child_entity).is_none());
        app.world_mut()
            .entity_mut(child_entity)
            .insert(Name::new("Edited"));
        app.world_mut().get_mut::<Revision>(child_entity).unwrap().0 = 8;
        app.world_mut().entity_mut(request).insert(CaptureRequest {
            scope_id: scope,
            entities: vec![child_entity, root_entity],
        });
        app.update();
        let Captured(Ok(captured)) = app.world().get::<Captured>(request).unwrap() else {
            panic!("expected capture")
        };
        assert_eq!(captured[0].id, child);
        assert_eq!(captured[0].relationships[0].target, root);
        assert_eq!(
            captured[0]
                .components
                .iter()
                .find(|component| component.key == "name")
                .unwrap()
                .value,
            serde_json::json!("Edited")
        );
        assert!(captured[1].relationships.is_empty());
        app.world_mut()
            .entity_mut(child_entity)
            .insert(Name::new("Newer edit"));
        app.world_mut().get_mut::<Revision>(child_entity).unwrap().0 = 9;
        app.world_mut().entity_mut(request).insert(Confirmed(vec![(
            child_entity,
            Receipt {
                operation_id: Uuid::new_v4(),
                entity_id: child,
                revision: 8,
            },
        )]));
        app.update();
        assert!(app.world().get::<Dirty>(child_entity).is_some());
        assert_eq!(app.world().get::<Persisted>(child_entity).unwrap().0, 8);
        for receipt in [
            Receipt {
                operation_id: Uuid::new_v4(),
                entity_id: Uuid::new_v4(),
                revision: 9,
            },
            Receipt {
                operation_id: Uuid::nil(),
                entity_id: child,
                revision: 9,
            },
        ] {
            app.world_mut()
                .entity_mut(request)
                .insert(Confirmed(vec![(child_entity, receipt)]));
            app.update();
            assert!(app.world().get::<Dirty>(child_entity).is_some());
            assert_eq!(app.world().get::<Persisted>(child_entity).unwrap().0, 8);
        }
        app.world_mut().entity_mut(request).insert(Confirmed(vec![(
            child_entity,
            Receipt {
                operation_id: Uuid::new_v4(),
                entity_id: child,
                revision: 9,
            },
        )]));
        app.update();
        assert!(app.world().get::<Dirty>(child_entity).is_none());
        app.world_mut().entity_mut(request).despawn();
        assert!(app.world().get_entity(root_entity).is_err());
        assert!(app.world().get_entity(child_entity).is_err());
    }

    #[test]
    fn graph_restore_rejects_cycles_missing_targets_duplicate_ids_and_mixed_scopes_without_leaks() {
        let scope = Uuid::new_v4();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let mut unknown = Record::fixture(scope, b, "Unknown", None);
        unknown.components[0].key = "unknown".to_string();
        let mut wrong_version = Record::fixture(scope, b, "Unsupported", None);
        wrong_version.components[0].version = 2;
        let mut invalid_value = Record::fixture(scope, b, "Invalid", None);
        invalid_value.components[0].value = serde_json::json!([]);
        for records in [
            vec![
                Record::fixture(scope, a, "A", Some(b)),
                Record::fixture(scope, b, "B", Some(a)),
            ],
            vec![Record::fixture(scope, a, "A", Some(b))],
            vec![
                Record::fixture(scope, a, "A", None),
                Record::fixture(scope, a, "Duplicate", None),
            ],
            vec![
                Record::fixture(scope, a, "A", None),
                Record::fixture(Uuid::new_v4(), b, "Other scope", None),
            ],
            vec![Record::fixture(scope, a, "A", None), unknown],
            vec![Record::fixture(scope, a, "A", None), wrong_version],
            vec![Record::fixture(scope, a, "A", None), invalid_value],
        ] {
            let mut app = App::new();
            app.add_plugins(Plugin).insert_resource(Schema::fixture());
            let request = app.world_mut().spawn(RestoreRequest(Graph(records))).id();
            app.update();
            assert!(matches!(
                app.world().get::<Restored>(request),
                Some(Restored(Err(_)))
            ));
            let mut identities = app.world_mut().query::<&Id>();
            assert_eq!(identities.iter(app.world()).count(), 0);
        }
    }

    #[test]
    fn records_and_receipts_preserve_the_existing_json_wire_contract() {
        let value = serde_json::json!({
            "scope_id": "11111111-1111-4111-8111-111111111111",
            "id": "22222222-2222-4222-8222-222222222222",
            "revision": 7,
            "deleted": false,
            "components": [{"key": "preferences", "version": 2, "value": {
                "fonts": [{"family": "Mono", "size": 14}]
            }}],
            "relationships": [{
                "key": "account", "version": 1,
                "target": "33333333-3333-4333-8333-333333333333",
                "position": 0, "owned": true
            }]
        });
        let record: Record = serde_json::from_value(value.clone()).unwrap();
        assert!(record.is_valid());
        assert_eq!(serde_json::to_value(record).unwrap(), value);
        let value = serde_json::json!({
            "operation_id": "44444444-4444-4444-8444-444444444444",
            "entity_id": "22222222-2222-4222-8222-222222222222",
            "revision": 8
        });
        let receipt: Receipt = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(receipt).unwrap(), value);
    }
}
