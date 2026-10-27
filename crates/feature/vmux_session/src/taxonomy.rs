use std::collections::{HashMap, HashSet};

#[cfg(host)]
use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
use vmux_api::Id;
use vmux_api::session::{Member as SerializedMember, Session};
use vmux_ecs::UnixMillis;
#[cfg(host)]
use vmux_ecs::persistence::PersistenceAppExt;

#[cfg(host)]
pub(crate) struct TaxonomyPlugin;

#[cfg(host)]
impl Plugin for TaxonomyPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::mutation::MutationPlugin>() {
            app.add_plugins(crate::mutation::MutationPlugin);
        }
        app.add_plugins((
            vmux_ecs::IdentityIndexPlugin::<Label>::default(),
            vmux_ecs::IdentityIndexPlugin::<Cycle>::default(),
        ))
        .register_persisted::<Label>()
        .register_persisted::<Id<Label>>()
        .register_persisted::<LabelColor>()
        .register_persisted::<Cycle>()
        .register_persisted::<Id<Cycle>>()
        .register_persisted::<CycleStartsAt>()
        .register_persisted::<CycleEndsAt>()
        .register_persisted::<CycleRef>()
        .register_persisted::<AssigneeRef>()
        .register_persisted::<LabelAssignment>()
        .register_persisted::<LabelRef>()
        .register_persisted::<Priority>()
        .register_persisted::<Estimate>()
        .register_persisted::<DueAt>()
        .add_message::<LabelSaveRequest>()
        .add_message::<CycleSaveRequest>()
        .add_message::<TaxonomyUpdateRequest>()
        .add_systems(
            Update,
            (save_labels, save_cycles).in_set(crate::model::MutationSet),
        )
        .add_systems(
            Update,
            update_taxonomy.in_set(crate::SessionMutationReceiveSet),
        )
        .add_systems(
            PostUpdate,
            (
                bevy_ecs::schedule::ApplyDeferred,
                rebuild_label_index,
                rebuild_cycle_index,
                resolve_cycle_targets,
                resolve_assignee_targets,
                resolve_label_targets,
            )
                .chain()
                .after(crate::model::CanonicalizeSet),
        );
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct Label;

#[derive(Component, Clone, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct LabelColor(String);

impl LabelColor {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let valid = value.len() == 7
            && value.starts_with('#')
            && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit());
        if !valid {
            return Err("label color must be #RRGGBB".to_string());
        }
        Ok(Self(value.to_ascii_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Bundle)]
pub struct LabelBundle {
    label: Label,
    id: Id<Label>,
    name: Name,
    color: LabelColor,
}

impl LabelBundle {
    pub fn new(id: Id<Label>, name: impl Into<String>, color: LabelColor) -> Result<Self, String> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err("label name must not be empty".to_string());
        }
        Ok(Self {
            label: Label,
            id,
            name: Name::new(name),
            color,
        })
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct Cycle;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct CycleStartsAt(UnixMillis);

impl CycleStartsAt {
    pub const fn new(milliseconds: i64) -> Self {
        Self(UnixMillis(milliseconds))
    }

    pub const fn milliseconds(self) -> i64 {
        self.0.0
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct CycleEndsAt(UnixMillis);

impl CycleEndsAt {
    pub const fn new(milliseconds: i64) -> Self {
        Self(UnixMillis(milliseconds))
    }

    pub const fn milliseconds(self) -> i64 {
        self.0.0
    }
}

#[derive(Bundle)]
pub struct CycleBundle {
    cycle: Cycle,
    id: Id<Cycle>,
    name: Name,
    starts_at: CycleStartsAt,
    ends_at: CycleEndsAt,
}

impl CycleBundle {
    pub fn new(
        id: Id<Cycle>,
        name: impl Into<String>,
        starts_at: i64,
        ends_at: i64,
    ) -> Result<Self, String> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err("cycle name must not be empty".to_string());
        }
        if ends_at <= starts_at {
            return Err("cycle end must be after its start".to_string());
        }
        Ok(Self {
            cycle: Cycle,
            id,
            name: Name::new(name),
            starts_at: CycleStartsAt::new(starts_at),
            ends_at: CycleEndsAt::new(ends_at),
        })
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct CycleRef(Id<Cycle>);

impl CycleRef {
    pub fn new(id: Id<Cycle>) -> Self {
        Self(id)
    }

    pub fn id(&self) -> &Id<Cycle> {
        &self.0
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = CycleSessions)]
pub struct CycleTarget(Entity);

impl CycleTarget {
    pub const fn new(entity: Entity) -> Self {
        Self(entity)
    }

    pub const fn entity(&self) -> Entity {
        self.0
    }
}

#[derive(Component, Debug)]
#[relationship_target(relationship = CycleTarget)]
pub struct CycleSessions(Vec<Entity>);

#[derive(Component, Clone, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct AssigneeRef(Id<SerializedMember>);

impl AssigneeRef {
    pub fn new(id: Id<SerializedMember>) -> Self {
        Self(id)
    }

    pub fn id(&self) -> &Id<SerializedMember> {
        &self.0
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = AssignedSessions)]
pub struct AssigneeTarget(Entity);

impl AssigneeTarget {
    pub const fn new(entity: Entity) -> Self {
        Self(entity)
    }

    pub const fn entity(&self) -> Entity {
        self.0
    }
}

#[derive(Component, Debug)]
#[relationship_target(relationship = AssigneeTarget)]
pub struct AssignedSessions(Vec<Entity>);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct LabelAssignment;

#[derive(Component, Clone, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct LabelRef(Id<Label>);

impl LabelRef {
    pub fn new(id: Id<Label>) -> Self {
        Self(id)
    }

    pub fn id(&self) -> &Id<Label> {
        &self.0
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = LabelAssignments)]
pub struct LabelTarget(Entity);

impl LabelTarget {
    pub const fn new(entity: Entity) -> Self {
        Self(entity)
    }

    pub const fn entity(&self) -> Entity {
        self.0
    }
}

#[derive(Component, Debug)]
#[relationship_target(relationship = LabelTarget)]
pub struct LabelAssignments(Vec<Entity>);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
#[type_path = "vmux_session"]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Urgent,
    High,
    Medium,
    Low,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct Estimate(u32);

impl Estimate {
    pub fn new(points: u32) -> Result<Self, String> {
        if points == 0 {
            return Err("estimate must be positive".to_string());
        }
        Ok(Self(points))
    }

    pub const fn points(self) -> u32 {
        self.0
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct DueAt(UnixMillis);

impl DueAt {
    pub const fn new(milliseconds: i64) -> Self {
        Self(UnixMillis(milliseconds))
    }

    pub const fn milliseconds(self) -> i64 {
        self.0.0
    }
}

#[cfg(host)]
#[derive(Message, Clone, Debug)]
pub struct LabelSaveRequest {
    id: Id<Label>,
    name: String,
    color: LabelColor,
}

#[cfg(host)]
impl LabelSaveRequest {
    pub fn new(id: Id<Label>, name: impl Into<String>, color: LabelColor) -> Result<Self, String> {
        let name = name.into();
        LabelBundle::new(id.clone(), name.clone(), color.clone())?;
        Ok(Self { id, name, color })
    }
}

#[cfg(host)]
#[derive(Message, Clone, Debug)]
pub struct CycleSaveRequest {
    id: Id<Cycle>,
    name: String,
    starts_at: i64,
    ends_at: i64,
}

#[cfg(host)]
impl CycleSaveRequest {
    pub fn new(
        id: Id<Cycle>,
        name: impl Into<String>,
        starts_at: i64,
        ends_at: i64,
    ) -> Result<Self, String> {
        let name = name.into();
        CycleBundle::new(id.clone(), name.clone(), starts_at, ends_at)?;
        Ok(Self {
            id,
            name,
            starts_at,
            ends_at,
        })
    }
}

#[cfg(host)]
#[derive(Message, Clone, Debug)]
pub struct TaxonomyUpdateRequest {
    session: Id<Session>,
    cycle: Option<Id<Cycle>>,
    assignee: Option<Id<SerializedMember>>,
    labels: Vec<Id<Label>>,
    priority: Option<Priority>,
    estimate: Option<Estimate>,
    due_at: Option<DueAt>,
}

#[derive(Clone, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[type_path = "vmux_session"]
pub struct TaxonomyMutation {
    cycle: Option<Id<Cycle>>,
    assignee: Option<Id<SerializedMember>>,
    labels: Vec<Id<Label>>,
    priority: Option<Priority>,
    estimate: Option<Estimate>,
    due_at: Option<DueAt>,
}

impl TaxonomyMutation {
    pub(crate) fn apply(
        &self,
        session: Entity,
        children: Option<&Children>,
        assignments: &Query<(Entity, &LabelRef), With<LabelAssignment>>,
        commands: &mut Commands,
    ) {
        let mut entity = commands.entity(session);
        match &self.cycle {
            Some(cycle) => {
                entity.insert(CycleRef::new(cycle.clone()));
            }
            None => {
                entity.remove::<(CycleRef, CycleTarget)>();
            }
        }
        match &self.assignee {
            Some(assignee) => {
                entity.insert(AssigneeRef::new(assignee.clone()));
            }
            None => {
                entity.remove::<(AssigneeRef, AssigneeTarget)>();
            }
        }
        match self.priority {
            Some(priority) => {
                entity.insert(priority);
            }
            None => {
                entity.remove::<Priority>();
            }
        }
        match self.estimate {
            Some(estimate) => {
                entity.insert(estimate);
            }
            None => {
                entity.remove::<Estimate>();
            }
        }
        match self.due_at {
            Some(due_at) => {
                entity.insert(due_at);
            }
            None => {
                entity.remove::<DueAt>();
            }
        }

        let requested = self.labels.iter().cloned().collect::<HashSet<_>>();
        let mut existing = HashMap::<Id<Label>, Entity>::new();
        for child in children.into_iter().flat_map(|children| children.iter()) {
            if let Ok((assignment, label)) = assignments.get(child) {
                existing.insert(label.id().clone(), assignment);
            }
        }
        for (label, assignment) in &existing {
            if !requested.contains(label) {
                commands.entity(*assignment).despawn();
            }
        }
        for label in &self.labels {
            if !existing.contains_key(label) {
                commands.spawn((
                    LabelAssignment,
                    LabelRef::new(label.clone()),
                    ChildOf(session),
                ));
            }
        }
    }
}

#[cfg(host)]
impl TaxonomyUpdateRequest {
    pub fn new(session: Id<Session>) -> Self {
        Self {
            session,
            cycle: None,
            assignee: None,
            labels: Vec::new(),
            priority: None,
            estimate: None,
            due_at: None,
        }
    }

    pub fn with_cycle(mut self, cycle: Id<Cycle>) -> Self {
        self.cycle = Some(cycle);
        self
    }

    pub fn with_assignee(mut self, assignee: Id<SerializedMember>) -> Self {
        self.assignee = Some(assignee);
        self
    }

    pub fn with_labels(mut self, mut labels: Vec<Id<Label>>) -> Self {
        labels.sort();
        labels.dedup();
        self.labels = labels;
        self
    }

    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = Some(priority);
        self
    }

    pub fn with_estimate(mut self, estimate: Estimate) -> Self {
        self.estimate = Some(estimate);
        self
    }

    pub fn with_due_at(mut self, due_at: DueAt) -> Self {
        self.due_at = Some(due_at);
        self
    }

    fn mutation(&self) -> TaxonomyMutation {
        TaxonomyMutation {
            cycle: self.cycle.clone(),
            assignee: self.assignee.clone(),
            labels: self.labels.clone(),
            priority: self.priority,
            estimate: self.estimate,
            due_at: self.due_at,
        }
    }
}

#[cfg(host)]
fn save_labels(
    mut requests: MessageReader<LabelSaveRequest>,
    index: Res<vmux_ecs::IdentityIndex<Label>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let entity = index.entity(&request.id).ok().flatten();
        if let Some(entity) = entity {
            commands
                .entity(entity)
                .insert((Name::new(request.name.clone()), request.color.clone()));
            continue;
        }
        if let Ok(bundle) = LabelBundle::new(
            request.id.clone(),
            request.name.clone(),
            request.color.clone(),
        ) {
            commands.spawn(bundle);
        }
    }
}

#[cfg(host)]
fn save_cycles(
    mut requests: MessageReader<CycleSaveRequest>,
    index: Res<vmux_ecs::IdentityIndex<Cycle>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let entity = index.entity(&request.id).ok().flatten();
        if let Some(entity) = entity {
            commands.entity(entity).insert((
                Name::new(request.name.clone()),
                CycleStartsAt::new(request.starts_at),
                CycleEndsAt::new(request.ends_at),
            ));
            continue;
        }
        if let Ok(bundle) = CycleBundle::new(
            request.id.clone(),
            request.name.clone(),
            request.starts_at,
            request.ends_at,
        ) {
            commands.spawn(bundle);
        }
    }
}

#[cfg(host)]
fn update_taxonomy(
    mut requests: MessageReader<TaxonomyUpdateRequest>,
    mut commits: MessageWriter<crate::SessionMutationRequest>,
) {
    for request in requests.read() {
        commits.write(crate::SessionMutationRequest::local(
            request.session.clone(),
            crate::SessionMutation::ChangeTaxonomy(request.mutation()),
        ));
    }
}

#[cfg(host)]
fn rebuild_label_index(mut index: vmux_ecs::IdentityIndexWriter<Label>) {
    index.rebuild();
}

#[cfg(host)]
fn rebuild_cycle_index(mut index: vmux_ecs::IdentityIndexWriter<Cycle>) {
    index.rebuild();
}

#[cfg(host)]
fn resolve_cycle_targets(
    index: Res<vmux_ecs::IdentityIndex<Cycle>>,
    references: Query<(Entity, Ref<CycleRef>, Option<&CycleTarget>)>,
    mut commands: Commands,
) {
    for (entity, reference, current) in &references {
        if !index.is_changed() && !reference.is_changed() {
            continue;
        }
        let target = index.entity(reference.id()).ok().flatten();
        match (target, current.map(CycleTarget::entity)) {
            (Some(target), Some(current)) if target == current => {}
            (Some(target), _) => {
                commands.entity(entity).insert(CycleTarget::new(target));
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<CycleTarget>();
            }
            (None, None) => {}
        }
    }
}

#[cfg(host)]
fn resolve_assignee_targets(
    index: Res<vmux_ecs::IdentityIndex<SerializedMember>>,
    references: Query<(Entity, Ref<AssigneeRef>, Option<&AssigneeTarget>)>,
    mut commands: Commands,
) {
    for (entity, reference, current) in &references {
        if !index.is_changed() && !reference.is_changed() {
            continue;
        }
        let target = index.entity(reference.id()).ok().flatten();
        match (target, current.map(AssigneeTarget::entity)) {
            (Some(target), Some(current)) if target == current => {}
            (Some(target), _) => {
                commands.entity(entity).insert(AssigneeTarget::new(target));
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<AssigneeTarget>();
            }
            (None, None) => {}
        }
    }
}

#[cfg(host)]
fn resolve_label_targets(
    index: Res<vmux_ecs::IdentityIndex<Label>>,
    references: Query<(Entity, Ref<LabelRef>, Option<&LabelTarget>), With<LabelAssignment>>,
    mut commands: Commands,
) {
    for (entity, reference, current) in &references {
        if !index.is_changed() && !reference.is_changed() {
            continue;
        }
        let target = index.entity(reference.id()).ok().flatten();
        match (target, current.map(LabelTarget::entity)) {
            (Some(target), Some(current)) if target == current => {}
            (Some(target), _) => {
                commands.entity(entity).insert(LabelTarget::new(target));
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<LabelTarget>();
            }
            (None, None) => {}
        }
    }
}

#[cfg(all(test, host))]
mod tests {
    use super::*;
    use vmux_api::session::{MemberKind, MemberRole};

    #[test]
    fn dynamic_taxonomy_resolves_session_relationships() {
        let mut app = App::new();
        app.add_plugins(crate::SessionDomainPlugin);
        let session_id = Id::<Session>::new("session-1");
        let session = app.world_mut().spawn((Session, session_id.clone())).id();
        let label_id = Id::<Label>::new("label-1");
        let cycle_id = Id::<Cycle>::new("cycle-1");
        app.world_mut()
            .resource_mut::<Messages<LabelSaveRequest>>()
            .write(
                LabelSaveRequest::new(label_id.clone(), "Bug", LabelColor::new("#ff0000").unwrap())
                    .unwrap(),
            );
        app.world_mut()
            .resource_mut::<Messages<CycleSaveRequest>>()
            .write(CycleSaveRequest::new(cycle_id.clone(), "v0.1", 1, 2).unwrap());

        app.update();

        let member_id = SerializedMember::local_id(&session_id);
        let member = app
            .world()
            .resource::<vmux_ecs::IdentityIndex<SerializedMember>>()
            .entity(&member_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            *app.world().get::<MemberRole>(member).unwrap(),
            MemberRole::Owner
        );
        assert_eq!(
            *app.world().get::<MemberKind>(member).unwrap(),
            MemberKind::Human
        );

        app.world_mut()
            .resource_mut::<Messages<TaxonomyUpdateRequest>>()
            .write(
                TaxonomyUpdateRequest::new(session_id)
                    .with_cycle(cycle_id)
                    .with_assignee(member_id)
                    .with_labels(vec![label_id])
                    .with_priority(Priority::Urgent)
                    .with_estimate(Estimate::new(3).unwrap())
                    .with_due_at(DueAt::new(3)),
            );

        app.update();

        assert_eq!(
            app.world().get::<AssigneeTarget>(session).unwrap().entity(),
            member
        );
        assert!(app.world().get::<CycleTarget>(session).is_some());
        assert_eq!(app.world().get::<Estimate>(session).unwrap().points(), 3);
        let assignments = app.world().get::<Children>(session).unwrap();
        assert!(assignments.iter().any(|child| {
            app.world()
                .get::<LabelTarget>(child)
                .is_some_and(|target| app.world().get::<Label>(target.entity()).is_some())
        }));
    }
}
