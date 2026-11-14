use crate::{AgentId, Route};
use bevy_app::{App, Plugin as BevyPlugin, Startup, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use std::collections::HashSet;
use std::path::PathBuf;
use vmux_api::command_bar::{AgentModels, AgentModes};
use vmux_api::protocol::AcpModeOption;
use vmux_api::session::ModelOptionEntry;
use vmux_ecs::{Active, BaseRevision, Cwd, Description, Order, Revision};

use crate::preferences_io::{Content, Plugin as IoPlugin};

pub struct Plugin;

impl BevyPlugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(IoPlugin)
            .add_message::<ProfileRequested>()
            .add_message::<RememberModel>()
            .add_message::<RememberMode>()
            .add_message::<RememberModels>()
            .add_message::<RememberModes>()
            .add_message::<RetryPreferences>()
            .add_observer(restore)
            .add_observer(drain)
            .add_observer(select_model)
            .add_observer(select_mode)
            .add_observer(replace_models)
            .add_observer(replace_modes)
            .add_systems(Startup, bind.in_set(ProfileSet))
            .add_systems(
                Update,
                bind.in_set(ProfileSet)
                    .before(crate::preferences_io::SubmitSet),
            )
            .add_systems(
                Update,
                retry.after(bind).before(crate::preferences_io::SubmitSet),
            )
            .add_systems(
                Update,
                (
                    enqueue::<RememberModel, false>,
                    enqueue::<RememberMode, true>,
                    enqueue::<RememberModels, false>,
                    enqueue::<RememberModes, true>,
                    consume,
                )
                    .chain()
                    .in_set(ModelSelectionSet),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModelSelectionSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProfileSet;

#[derive(Component)]
pub(crate) struct Root;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Models,
    Modes,
}

impl Kind {
    pub(crate) fn filename(self) -> &'static str {
        match self {
            Self::Models => "agent-models.json",
            Self::Modes => "agent-modes.json",
        }
    }
}

#[derive(Component)]
pub(crate) struct File {
    pub(crate) pending: Option<Entity>,
    pub(crate) retry_allowed: bool,
    next_order: u32,
}

#[derive(Component)]
pub(crate) struct FilePath(pub(crate) PathBuf);

#[derive(Component)]
pub(crate) struct Ready;

#[derive(Component)]
pub(crate) struct Preference;

#[derive(Component, Default)]
pub(crate) struct Selection(pub(crate) String);

#[derive(Component, Default)]
pub(crate) struct SourceUrl(pub(crate) String);

#[derive(Component)]
struct Choice;

#[derive(Component)]
struct OptionId(String);

#[derive(Component)]
struct Described;

#[derive(Component)]
pub(crate) struct Queued {
    consumed: bool,
}

#[derive(Message)]
pub struct ProfileRequested(pub PathBuf);

#[derive(Message)]
pub struct RetryPreferences {
    pub kind: Kind,
}

#[derive(Event)]
pub(crate) struct Loaded {
    pub(crate) file: Entity,
    pub(crate) content: Content,
}

#[derive(EntityEvent)]
pub(crate) struct Drain {
    #[event_target]
    pub(crate) file: Entity,
}

#[derive(EntityEvent)]
struct Apply<T: Component> {
    #[event_target]
    file: Entity,
    request: T,
}

#[derive(Component, Message, Clone)]
pub struct RememberModel {
    pub agent_id: String,
    pub model_id: String,
}

#[derive(Component, Message, Clone)]
pub struct RememberMode {
    pub agent_id: String,
    pub mode_id: String,
}

#[derive(Component, Message, Clone)]
pub struct RememberModels {
    pub agent_id: String,
    pub url: String,
    pub selected: String,
    pub models: Vec<ModelOptionEntry>,
}

#[derive(Component, Message, Clone)]
pub struct RememberModes {
    pub agent_id: String,
    pub url: String,
    pub selected: String,
    pub modes: Vec<AcpModeOption>,
}

fn bind(
    mut requests: MessageReader<ProfileRequested>,
    roots: Query<(Entity, &Cwd, Has<Active>), With<Root>>,
    mut commands: Commands,
) {
    let Some(request) = requests.read().last() else {
        return;
    };
    let directory = vmux_path::PathIdentity::resolve(&request.0).into_path_buf();
    let mut selected = None;
    for (entity, cwd, active) in &roots {
        if cwd.0 == directory {
            selected = Some(entity);
            if !active {
                commands.entity(entity).insert(Active);
            }
        } else if active {
            commands.entity(entity).remove::<Active>();
        }
    }
    if selected.is_some() {
        return;
    }
    let root = commands
        .spawn((
            Root,
            Cwd(directory.clone()),
            Active,
            Name::new("Agent preferences"),
        ))
        .id();
    for kind in [Kind::Models, Kind::Modes] {
        commands.spawn((
            File {
                pending: None,
                retry_allowed: true,
                next_order: 0,
            },
            kind,
            FilePath(directory.join(kind.filename())),
            Revision::default(),
            BaseRevision(0),
            Name::new(kind.filename()),
            ChildOf(root),
        ));
    }
}

#[derive(SystemParam)]
pub struct AgentPreferences<'w, 's> {
    failures: Query<'w, 's, &'static vmux_ecs::OperationError, With<File>>,
    roots: Query<'w, 's, (Entity, Ref<'static, Active>, &'static Cwd), With<Root>>,
    files: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static Kind,
            Ref<'static, Revision>,
            Option<Ref<'static, Ready>>,
        ),
        With<File>,
    >,
    entries: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static AgentId,
            &'static Selection,
            &'static SourceUrl,
        ),
        With<Preference>,
    >,
    options: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static OptionId,
            &'static Name,
            &'static Description,
            &'static Order,
            Has<Described>,
        ),
        With<Choice>,
    >,
}

impl AgentPreferences<'_, '_> {
    fn root(&self) -> Option<Entity> {
        self.roots.single().ok().map(|(entity, _, _)| entity)
    }

    pub fn profile_matches(&self, directory: &std::path::Path) -> bool {
        self.roots
            .single()
            .is_ok_and(|(_, _, cwd)| cwd.0 == directory)
    }

    pub fn file(&self, kind: Kind) -> Option<Entity> {
        for (entity, parent, candidate, _, _) in &self.files {
            if *candidate == kind && self.root() == Some(parent.parent()) {
                return Some(entity);
            }
        }
        None
    }

    pub fn ready(&self, kind: Kind) -> bool {
        let Some(file) = self.file(kind) else {
            return false;
        };
        self.files
            .get(file)
            .is_ok_and(|(_, _, _, _, ready)| ready.is_some())
    }

    pub fn load_error(&self, kind: Kind) -> Option<(Entity, &str)> {
        if self.ready(kind) {
            return None;
        }
        let file = self.file(kind)?;
        let error = self.failures.get(file).ok()?;
        Some((file, error.message()))
    }

    pub fn changed(&self, kind: Kind) -> bool {
        let Some(file) = self.file(kind) else {
            return false;
        };
        let Ok((_, parent, _, revision, ready)) = self.files.get(file) else {
            return false;
        };
        revision.is_changed()
            || ready.is_some_and(|ready| ready.is_changed())
            || self
                .roots
                .get(parent.parent())
                .is_ok_and(|(_, active, _)| active.is_changed())
    }

    pub fn model(&self, agent: &str) -> Option<&str> {
        self.selected(Kind::Models, agent)
    }

    pub fn mode(&self, agent: &str) -> Option<&str> {
        self.selected(Kind::Modes, agent)
    }

    fn selected(&self, kind: Kind, agent: &str) -> Option<&str> {
        let file = self.file(kind)?;
        if !self.ready(kind) {
            return None;
        }
        for (_, parent, id, selection, _) in &self.entries {
            if parent.parent() == file && id.0 == agent {
                return Some(&selection.0);
            }
        }
        None
    }

    pub(crate) fn records(&self, file: Entity) -> Vec<(Entity, &AgentId, &Selection, &SourceUrl)> {
        let mut records = Vec::new();
        for (entity, parent, agent, selected, url) in &self.entries {
            if parent.parent() == file {
                records.push((entity, agent, selected, url));
            }
        }
        records
    }

    fn options(&self, entity: Entity) -> Vec<Entity> {
        let mut ordered = Vec::new();
        for (option, parent, _, _, _, order, _) in &self.options {
            if parent.parent() == entity {
                ordered.push((order.0, option));
            }
        }
        ordered.sort_unstable_by_key(|(order, _)| *order);
        let mut entities = Vec::with_capacity(ordered.len());
        for (_, entity) in ordered {
            entities.push(entity);
        }
        entities
    }

    pub(crate) fn model_options(&self, entity: Entity) -> Vec<ModelOptionEntry> {
        let mut result = Vec::new();
        for option in self.options(entity) {
            let Ok((_, _, id, name, description, _, _)) = self.options.get(option) else {
                continue;
            };
            result.push(ModelOptionEntry {
                id: id.0.clone(),
                name: name.as_str().to_string(),
                description: description.0.clone(),
            });
        }
        result
    }

    pub(crate) fn mode_options(&self, entity: Entity) -> Vec<AcpModeOption> {
        let mut result = Vec::new();
        for option in self.options(entity) {
            let Ok((_, _, id, name, description, _, described)) = self.options.get(option) else {
                continue;
            };
            result.push(AcpModeOption {
                id: id.0.clone(),
                name: name.as_str().to_string(),
                description: described.then(|| description.0.clone()),
            });
        }
        result
    }

    pub fn model_catalogs(&self) -> Vec<AgentModels> {
        let Some(file) = self.file(Kind::Models) else {
            return Vec::new();
        };
        let mut result = Vec::new();
        for (entity, agent, selected, url) in self.records(file) {
            let models = self.model_options(entity);
            if url.0.is_empty() || models.is_empty() {
                continue;
            }
            result.push(AgentModels {
                agent_key: agent.0.clone(),
                url: url.0.clone(),
                selected: selected.0.clone(),
                models,
            });
        }
        result.sort_by(|a, b| a.agent_key.cmp(&b.agent_key));
        result
    }

    pub fn mode_catalogs(&self) -> Vec<AgentModes> {
        let Some(file) = self.file(Kind::Modes) else {
            return Vec::new();
        };
        let mut result = Vec::new();
        for (entity, agent, selected, url) in self.records(file) {
            let modes = self.mode_options(entity);
            if url.0.is_empty() || modes.is_empty() {
                continue;
            }
            result.push(AgentModes {
                agent_key: agent.0.clone(),
                url: url.0.clone(),
                selected: selected.0.clone(),
                modes,
            });
        }
        result.sort_by(|a, b| a.agent_key.cmp(&b.agent_key));
        result
    }
}

fn retry(
    mut requests: MessageReader<RetryPreferences>,
    preferences: AgentPreferences,
    mut files: Query<&mut File>,
    wake: Option<Res<vmux_ecs::jobs::JobWake>>,
) {
    for request in requests.read() {
        let Some(file) = preferences.file(request.kind) else {
            continue;
        };
        let Ok(mut state) = files.get_mut(file) else {
            continue;
        };
        state.retry_allowed = true;
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn enqueue<T: Component + Message + Clone, const MODES: bool>(
    mut requests: MessageReader<T>,
    roots: Query<Entity, (With<Root>, With<Active>)>,
    mut files: Query<(Entity, &ChildOf, &Kind, &mut File, Has<Ready>)>,
    mut pending: Query<(Entity, &ChildOf, &mut Order), With<Queued>>,
    wake: Option<Res<vmux_ecs::jobs::JobWake>>,
    mut commands: Commands,
) {
    let Ok(root) = roots.single() else {
        return;
    };
    for request in requests.read() {
        let Some((file_entity, _, _, mut file, ready)) =
            files.iter_mut().find(|(_, parent, kind, _, _)| {
                parent.parent() == root && **kind == if MODES { Kind::Modes } else { Kind::Models }
            })
        else {
            continue;
        };
        if file.next_order == u32::MAX {
            let mut ordered = Vec::new();
            for (entity, parent, order) in &pending {
                if parent.parent() == file_entity {
                    ordered.push((order.0, entity));
                }
            }
            ordered.sort_unstable_by_key(|(order, _)| *order);
            let Ok(next_order) = u32::try_from(ordered.len()) else {
                continue;
            };
            if next_order == u32::MAX {
                continue;
            }
            for (index, (_, entity)) in ordered.into_iter().enumerate() {
                if let Ok((_, _, mut order)) = pending.get_mut(entity) {
                    order.0 = index as u32;
                }
            }
            file.next_order = next_order;
        }
        if !ready {
            file.retry_allowed = true;
        }
        let order = file.next_order;
        file.next_order += 1;
        commands.spawn((
            Queued { consumed: false },
            request.clone(),
            Order(order),
            ChildOf(file_entity),
        ));
        if !ready && let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn consume(preferences: AgentPreferences, mut commands: Commands) {
    for (file, parent, _, _, ready) in &preferences.files {
        if ready.is_some() && preferences.roots.contains(parent.parent()) {
            commands.trigger(Drain { file });
        }
    }
}

fn drain(
    event: On<Drain>,
    files: Query<&ChildOf, (With<File>, With<Ready>)>,
    roots: Query<(), With<Root>>,
    mut pending: Query<(
        Entity,
        &ChildOf,
        &Order,
        &mut Queued,
        Option<&RememberModel>,
        Option<&RememberMode>,
        Option<&RememberModels>,
        Option<&RememberModes>,
    )>,
    mut commands: Commands,
) {
    let Ok(parent) = files.get(event.file) else {
        return;
    };
    if !roots.contains(parent.parent()) {
        return;
    }
    let mut ordered = Vec::new();
    for (entity, parent, order, queued, _, _, models, modes) in &pending {
        if parent.parent() == event.file && !queued.consumed {
            ordered.push((models.is_none() && modes.is_none(), order.0, entity));
        }
    }
    ordered.sort_unstable_by_key(|(choice, order, _)| (*choice, *order));
    for (_, _, entity) in ordered {
        let Ok((_, _, _, mut queued, model, mode, models, modes)) = pending.get_mut(entity) else {
            continue;
        };
        if queued.consumed {
            continue;
        }
        queued.consumed = true;
        if let Some(request) = model {
            commands.trigger(Apply {
                file: event.file,
                request: request.clone(),
            });
        }
        if let Some(request) = mode {
            commands.trigger(Apply {
                file: event.file,
                request: request.clone(),
            });
        }
        if let Some(request) = models {
            commands.trigger(Apply {
                file: event.file,
                request: request.clone(),
            });
        }
        if let Some(request) = modes {
            commands.trigger(Apply {
                file: event.file,
                request: request.clone(),
            });
        }
        commands.entity(entity).despawn();
    }
}

#[derive(SystemParam)]
struct Writes<'w, 's> {
    roots: Query<'w, 's, (), With<Root>>,
    files: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static Kind,
            &'static mut File,
            &'static mut Revision,
        ),
    >,
    entries: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static AgentId,
            &'static mut Selection,
            &'static mut SourceUrl,
        ),
        With<Preference>,
    >,
    options: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static OptionId,
            &'static Name,
            &'static Description,
            &'static Order,
            Has<Described>,
        ),
        With<Choice>,
    >,
    commands: Commands<'w, 's>,
}

impl Writes<'_, '_> {
    fn accepts(&self, file: Entity, kind: Kind) -> bool {
        self.files
            .get(file)
            .is_ok_and(|(_, parent, candidate, _, revision)| {
                *candidate == kind
                    && self.roots.contains(parent.parent())
                    && revision.next().is_some()
            })
    }

    fn agent(&mut self, file: Entity, id: &str) -> Entity {
        for (entity, parent, agent, _, _) in &self.entries {
            if parent.parent() == file && agent.0 == id {
                return entity;
            }
        }
        self.commands
            .spawn((
                Preference,
                AgentId(id.to_string()),
                Selection::default(),
                SourceUrl::default(),
                Name::new(id.to_string()),
                ChildOf(file),
            ))
            .remove::<moonshine_save::prelude::Save>()
            .id()
    }

    fn selected(&self, entity: Entity) -> &str {
        self.entries
            .get(entity)
            .map_or("", |(_, _, _, selected, _)| selected.0.as_str())
    }

    fn contains(&self, entity: Entity, id: &str) -> bool {
        self.options
            .iter()
            .any(|(_, parent, option, ..)| parent.parent() == entity && option.0 == id)
    }

    fn has_options(&self, entity: Entity) -> bool {
        self.options
            .iter()
            .any(|(_, parent, ..)| parent.parent() == entity)
    }

    fn update(&mut self, entity: Entity, selected: &str, url: Option<&str>) -> bool {
        let Ok((_, _, _, mut current, mut source)) = self.entries.get_mut(entity) else {
            self.commands.entity(entity).insert((
                Selection(selected.to_string()),
                SourceUrl(url.unwrap_or("").to_string()),
            ));
            return true;
        };
        let mut changed = false;
        if current.0 != selected {
            current.0 = selected.to_string();
            changed = true;
        }
        if let Some(url) = url
            && source.0 != url
        {
            source.0 = url.to_string();
            changed = true;
        }
        changed
    }

    fn option(
        &mut self,
        parent: Entity,
        index: usize,
        id: &str,
        name: &str,
        description: Option<&str>,
        retained: &mut HashSet<Entity>,
    ) -> bool {
        for (entity, owner, current_id, current_name, current_description, order, described) in
            &self.options
        {
            if owner.parent() != parent || current_id.0 != id || retained.contains(&entity) {
                continue;
            }
            retained.insert(entity);
            if current_name.as_str() == name
                && current_description.0 == description.unwrap_or("")
                && order.0 as usize == index
                && described == description.is_some()
            {
                return false;
            }
            let mut target = self.commands.entity(entity);
            target.insert((
                Name::new(name.to_string()),
                Description(description.unwrap_or("").to_string()),
                Order(index as u32),
            ));
            if description.is_some() {
                target.insert(Described);
            } else {
                target.remove::<Described>();
            }
            return true;
        }
        let mut target = self.commands.spawn((
            Choice,
            OptionId(id.to_string()),
            Name::new(name.to_string()),
            Description(description.unwrap_or("").to_string()),
            Order(index as u32),
            ChildOf(parent),
        ));
        if description.is_some() {
            target.insert(Described);
        }
        retained.insert(target.id());
        true
    }

    fn remove_options(&mut self, parent: Entity, retained: &HashSet<Entity>) -> bool {
        let mut changed = false;
        for (entity, owner, ..) in &self.options {
            if owner.parent() == parent && !retained.contains(&entity) {
                self.commands.entity(entity).despawn();
                changed = true;
            }
        }
        changed
    }

    fn model_options(&mut self, parent: Entity, models: &[ModelOptionEntry]) -> bool {
        let mut retained = HashSet::new();
        let mut changed = false;
        for (index, model) in models.iter().enumerate() {
            changed |= self.option(
                parent,
                index,
                &model.id,
                &model.name,
                Some(&model.description),
                &mut retained,
            );
        }
        changed | self.remove_options(parent, &retained)
    }

    fn mode_options(&mut self, parent: Entity, modes: &[AcpModeOption]) -> bool {
        let mut retained = HashSet::new();
        let mut changed = false;
        for (index, mode) in modes.iter().enumerate() {
            changed |= self.option(
                parent,
                index,
                &mode.id,
                &mode.name,
                mode.description.as_deref(),
                &mut retained,
            );
        }
        changed | self.remove_options(parent, &retained)
    }

    fn accept(&mut self, file: Entity, changed: bool) {
        let Ok((_, _, _, mut state, mut revision)) = self.files.get_mut(file) else {
            return;
        };
        state.retry_allowed = true;
        if changed && let Some(next) = revision.next() {
            *revision = next;
        }
    }
}

fn select_model(event: On<Apply<RememberModel>>, mut writes: Writes) {
    let file = event.file;
    if !writes.accepts(file, Kind::Models) {
        return;
    }
    let entity = writes.agent(file, &event.request.agent_id);
    if writes.has_options(entity) && !writes.contains(entity, &event.request.model_id) {
        return;
    }
    let changed = writes.update(entity, &event.request.model_id, None);
    writes.accept(file, changed);
}

fn select_mode(event: On<Apply<RememberMode>>, mut writes: Writes) {
    let file = event.file;
    if !writes.accepts(file, Kind::Modes) {
        return;
    }
    let entity = writes.agent(file, &event.request.agent_id);
    if writes.has_options(entity) && !writes.contains(entity, &event.request.mode_id) {
        return;
    }
    let changed = writes.update(entity, &event.request.mode_id, None);
    writes.accept(file, changed);
}

fn replace_models(event: On<Apply<RememberModels>>, mut writes: Writes) {
    if event.request.models.is_empty() {
        return;
    }
    let file = event.file;
    if !writes.accepts(file, Kind::Models) {
        return;
    }
    let entity = writes.agent(file, &event.request.agent_id);
    let previous = writes.selected(entity).to_string();
    let selected = if event
        .request
        .models
        .iter()
        .any(|model| model.id == previous)
    {
        previous.as_str()
    } else if event
        .request
        .models
        .iter()
        .any(|model| model.id == event.request.selected)
    {
        event.request.selected.as_str()
    } else {
        event.request.models[0].id.as_str()
    };
    let changed = writes.update(entity, selected, Some(&event.request.url))
        | writes.model_options(entity, &event.request.models);
    writes.accept(file, changed);
}

fn replace_modes(event: On<Apply<RememberModes>>, mut writes: Writes) {
    if event.request.modes.is_empty() {
        return;
    }
    let file = event.file;
    if !writes.accepts(file, Kind::Modes) {
        return;
    }
    let entity = writes.agent(file, &event.request.agent_id);
    let previous = writes.selected(entity).to_string();
    let selected = if event.request.modes.iter().any(|mode| mode.id == previous) {
        previous.as_str()
    } else if event
        .request
        .modes
        .iter()
        .any(|mode| mode.id == event.request.selected)
    {
        event.request.selected.as_str()
    } else {
        event.request.modes[0].id.as_str()
    };
    let changed = writes.update(entity, selected, Some(&event.request.url))
        | writes.mode_options(entity, &event.request.modes);
    writes.accept(file, changed);
}

fn restore(event: On<Loaded>, mut writes: Writes) {
    let kind = match &event.content {
        Content::Models(_) => Kind::Models,
        Content::Modes(_) => Kind::Modes,
        Content::Written => return,
    };
    if !writes.accepts(event.file, kind) {
        return;
    }
    match &event.content {
        Content::Models(records) => {
            for (agent, record) in records {
                let entity = writes.agent(event.file, agent);
                let url = if !record.url.is_empty() && Route::parse(&record.url).is_none() {
                    Route::manager_for_agent(&AgentId(agent.clone()))
                } else {
                    record.url.clone()
                };
                writes.update(entity, &record.selected, Some(&url));
                writes.model_options(entity, &record.models);
            }
        }
        Content::Modes(records) => {
            for (agent, record) in records {
                let entity = writes.agent(event.file, agent);
                let url = if !record.url.is_empty() && Route::parse(&record.url).is_none() {
                    String::new()
                } else {
                    record.url.clone()
                };
                writes.update(entity, &record.selected, Some(&url));
                writes.mode_options(entity, &record.modes);
            }
        }
        Content::Written => {}
    }
    writes.commands.entity(event.file).insert(Ready);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_app::AppExit;
    use bevy_ecs::system::RunSystemOnce;
    use std::time::{Duration, Instant};
    use vmux_ecs::OperationState;

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
    }

    impl Fixture {
        fn new(models: Option<&str>, modes: Option<&str>) -> Self {
            let directory = tempfile::tempdir().unwrap();
            for (kind, data) in [(Kind::Models, models), (Kind::Modes, modes)] {
                if let Some(data) = data {
                    std::fs::write(directory.path().join(kind.filename()), data).unwrap();
                }
            }
            let mut app = App::new();
            app.add_plugins(super::Plugin);
            app.world_mut()
                .write_message(ProfileRequested(directory.path().to_path_buf()));
            Self { app, directory }
        }

        fn send<T: Message>(&mut self, request: T) {
            self.app.world_mut().write_message(request);
        }

        fn settle(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                let ready = self
                    .app
                    .world_mut()
                    .run_system_once(|preferences: AgentPreferences| {
                        preferences.ready(Kind::Models) && preferences.ready(Kind::Modes)
                    })
                    .unwrap();
                let mut files = self
                    .app
                    .world_mut()
                    .query::<(&File, &Revision, &BaseRevision)>();
                let settled = files
                    .iter(self.app.world())
                    .all(|(file, current, saved)| file.pending.is_none() && current.0 == saved.0);
                if ready && settled {
                    return;
                }
                assert!(Instant::now() < deadline, "preference IO did not settle");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn model(&mut self, agent: &str) -> Option<String> {
            let agent = agent.to_string();
            self.app
                .world_mut()
                .run_system_once(move |preferences: AgentPreferences| {
                    preferences.model(&agent).map(str::to_string)
                })
                .unwrap()
        }

        fn mode(&mut self, agent: &str) -> Option<String> {
            let agent = agent.to_string();
            self.app
                .world_mut()
                .run_system_once(move |preferences: AgentPreferences| {
                    preferences.mode(&agent).map(str::to_string)
                })
                .unwrap()
        }

        fn stored(&self, kind: Kind) -> serde_json::Value {
            serde_json::from_slice(
                &std::fs::read(self.directory.path().join(kind.filename())).unwrap(),
            )
            .unwrap()
        }
    }

    #[test]
    fn legacy_files_materialize_before_consumers_and_keep_agent_scope() {
        let models = r#"{"one":{"url":"invalid://old","selected":"kept","models":[{"id":"kept","name":"Kept"}]}}"#;
        let modes = r#"{"one":{"url":"invalid://old","selected":"ask","modes":[{"id":"ask","name":"Ask","description":null}]},"two":{"url":"","selected":"auto","modes":[]}}"#;
        let mut fixture = Fixture::new(Some(models), Some(modes));
        fixture.settle();
        assert_eq!(fixture.model("one").as_deref(), Some("kept"));
        assert_eq!(fixture.mode("one").as_deref(), Some("ask"));
        assert_eq!(fixture.mode("two").as_deref(), Some("auto"));
        assert!(fixture.mode("missing").is_none());
        let catalogs = fixture
            .app
            .world_mut()
            .run_system_once(|preferences: AgentPreferences| preferences.model_catalogs())
            .unwrap();
        assert_eq!(catalogs[0].url, "vmux://sessions/inbox?agent=one");
        let mut saved = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, (With<Preference>, With<moonshine_save::prelude::Save>)>();
        assert_eq!(saved.iter(fixture.app.world()).count(), 0);
    }

    #[test]
    fn retained_inputs_use_current_options_and_the_latest_explicit_choice() {
        let mut fixture = Fixture::new(None, None);
        fixture.send(RememberModel {
            agent_id: "one".into(),
            model_id: "new".into(),
        });
        fixture.send(RememberModels {
            agent_id: "one".into(),
            url: "vmux://sessions/inbox?agent=one".into(),
            selected: "old".into(),
            models: vec![
                ModelOptionEntry {
                    id: "old".into(),
                    name: "Old".into(),
                    description: String::new(),
                },
                ModelOptionEntry {
                    id: "new".into(),
                    name: "New".into(),
                    description: String::new(),
                },
            ],
        });
        fixture.settle();
        assert_eq!(fixture.model("one").as_deref(), Some("new"));
        assert_eq!(fixture.stored(Kind::Models)["one"]["selected"], "new");
        fixture.send(RememberModel {
            agent_id: "one".into(),
            model_id: "missing".into(),
        });
        fixture.app.update();
        assert_eq!(fixture.model("one").as_deref(), Some("new"));
        fixture.send(RememberModels {
            agent_id: "one".into(),
            url: "vmux://sessions/inbox?agent=one".into(),
            selected: "fallback".into(),
            models: vec![ModelOptionEntry {
                id: "fallback".into(),
                name: "Fallback".into(),
                description: String::new(),
            }],
        });
        fixture.settle();
        assert_eq!(fixture.model("one").as_deref(), Some("fallback"));
    }

    #[test]
    fn mode_roundtrip_preserves_order_and_optional_descriptions() {
        let mut fixture = Fixture::new(None, None);
        fixture.settle();
        fixture.send(RememberModes {
            agent_id: "one".into(),
            url: "vmux://sessions/inbox?agent=one".into(),
            selected: "second".into(),
            modes: vec![
                AcpModeOption {
                    id: "second".into(),
                    name: "Second".into(),
                    description: None,
                },
                AcpModeOption {
                    id: "first".into(),
                    name: "First".into(),
                    description: Some(String::new()),
                },
            ],
        });
        fixture.settle();
        let stored = fixture.stored(Kind::Modes);
        assert_eq!(stored["one"]["selected"], "second");
        assert_eq!(stored["one"]["modes"][0]["id"], "second");
        assert!(stored["one"]["modes"][0]["description"].is_null());
        assert_eq!(stored["one"]["modes"][1]["description"], "");
        let mut restored = Fixture::new(None, Some(&stored.to_string()));
        restored.settle();
        assert_eq!(restored.mode("one").as_deref(), Some("second"));
    }

    #[test]
    fn quitting_flushes_the_latest_choice_before_an_older_queued_write() {
        let mut fixture = Fixture::new(None, None);
        fixture.settle();
        fixture.send(RememberMode {
            agent_id: "one".into(),
            mode_id: "first".into(),
        });
        fixture.app.update();
        fixture.send(RememberMode {
            agent_id: "one".into(),
            mode_id: "last".into(),
        });
        fixture.app.world_mut().write_message(AppExit::Success);
        fixture.app.update();
        assert_eq!(fixture.stored(Kind::Modes)["one"]["selected"], "last");
    }

    #[test]
    fn failed_writes_retain_the_choice_and_rearm_on_a_later_input() {
        let modes = r#"{"one":{"url":"","selected":"old","modes":[{"id":"old","name":"Old"},{"id":"new","name":"New"}]}}"#;
        let mut fixture = Fixture::new(None, Some(modes));
        fixture.settle();
        let lock = fixture.directory.path().join(".agent-modes.json.lock");
        std::fs::remove_file(&lock).unwrap();
        std::fs::create_dir(&lock).unwrap();
        fixture.send(RememberMode {
            agent_id: "one".into(),
            mode_id: "new".into(),
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture.app.update();
            let mut failed = fixture
                .app
                .world_mut()
                .query_filtered::<&OperationState, With<vmux_ecs::OperationError>>();
            if failed
                .iter(fixture.app.world())
                .any(|state| *state == OperationState::Failed)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "failed write was not acknowledged"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(fixture.mode("one").as_deref(), Some("new"));
        assert_eq!(fixture.stored(Kind::Modes)["one"]["selected"], "old");
        let mut failures = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<vmux_ecs::OperationError>>();
        let before = failures.iter(fixture.app.world()).count();
        for _ in 0..5 {
            fixture.app.update();
        }
        assert_eq!(failures.iter(fixture.app.world()).count(), before);
        std::fs::remove_dir(&lock).unwrap();
        fixture.send(RememberMode {
            agent_id: "one".into(),
            mode_id: "new".into(),
        });
        fixture.settle();
        assert_eq!(fixture.stored(Kind::Modes)["one"]["selected"], "new");
    }

    #[test]
    fn a_late_read_cannot_replace_the_new_profiles_preferences() {
        let models = r#"{"one":{"url":"","selected":"old","models":[]}}"#;
        let mut fixture = Fixture::new(Some(models), None);
        let lock_path = fixture.directory.path().join(".agent-models.json.lock");
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .unwrap();
        lock.lock().unwrap();
        fixture.app.update();
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("agent-models.json"),
            r#"{"one":{"url":"","selected":"new","models":[]}}"#,
        )
        .unwrap();
        fixture.send(ProfileRequested(directory.path().to_path_buf()));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture.app.update();
            if fixture.model("one").as_deref() == Some("new") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "replacement profile did not load"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        drop(lock);
        fixture.settle();
        assert_eq!(fixture.model("one").as_deref(), Some("new"));
    }

    #[test]
    fn failed_initial_read_keeps_inputs_and_preserves_other_agents_on_recovery() {
        let modes = r#"{"one":{"selected":"old","modes":[{"id":"old","name":"Old"},{"id":"new","name":"New"}]},"two":{"selected":"auto","modes":[]}}"#;
        let mut fixture = Fixture::new(None, Some(modes));
        let lock = fixture.directory.path().join(".agent-modes.json.lock");
        std::fs::create_dir(&lock).unwrap();
        fixture.send(RememberMode {
            agent_id: "one".into(),
            mode_id: "new".into(),
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture.app.update();
            let mut failures = fixture.app.world_mut().query::<&vmux_ecs::OperationError>();
            if failures.iter(fixture.app.world()).next().is_some() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "initial read failure was not reported"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(fixture.mode("one").is_none());
        assert_eq!(fixture.stored(Kind::Modes)["two"]["selected"], "auto");
        let mut queued = fixture.app.world_mut().query::<&RememberMode>();
        assert_eq!(queued.iter(fixture.app.world()).count(), 1);
        for _ in 0..5 {
            fixture.app.update();
        }
        assert_eq!(fixture.stored(Kind::Modes)["one"]["selected"], "old");
        std::fs::remove_dir(&lock).unwrap();
        fixture.send(RememberMode {
            agent_id: "one".into(),
            mode_id: "new".into(),
        });
        fixture.settle();
        assert_eq!(fixture.mode("one").as_deref(), Some("new"));
        assert_eq!(fixture.mode("two").as_deref(), Some("auto"));
        assert_eq!(fixture.stored(Kind::Modes)["one"]["selected"], "new");
        assert_eq!(fixture.stored(Kind::Modes)["two"]["selected"], "auto");
    }

    #[test]
    fn quitting_during_initial_loading_applies_retained_choices_before_saving() {
        let modes = r#"{"one":{"selected":"old","modes":[{"id":"old","name":"Old"},{"id":"new","name":"New"}]},"two":{"selected":"auto","modes":[]}}"#;
        let mut fixture = Fixture::new(None, Some(modes));
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(fixture.directory.path().join(".agent-modes.json.lock"))
            .unwrap();
        lock.lock().unwrap();
        fixture.send(RememberMode {
            agent_id: "one".into(),
            mode_id: "new".into(),
        });
        fixture.app.update();
        assert!(fixture.mode("one").is_none());
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            drop(lock);
        });
        fixture.app.world_mut().write_message(AppExit::Success);
        fixture.app.update();
        release.join().unwrap();
        assert_eq!(fixture.stored(Kind::Modes)["one"]["selected"], "new");
        assert_eq!(fixture.stored(Kind::Modes)["two"]["selected"], "auto");
    }

    #[test]
    fn switching_profiles_during_loading_retains_the_original_profiles_choices() {
        let modes = r#"{"one":{"selected":"old","modes":[{"id":"old","name":"Old"},{"id":"new","name":"New"}]}}"#;
        let mut fixture = Fixture::new(None, Some(modes));
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(fixture.directory.path().join(".agent-modes.json.lock"))
            .unwrap();
        lock.lock().unwrap();
        fixture.send(RememberMode {
            agent_id: "one".into(),
            mode_id: "new".into(),
        });
        fixture.app.update();
        let other = tempfile::tempdir().unwrap();
        std::fs::write(
            other.path().join("agent-modes.json"),
            r#"{"one":{"selected":"other","modes":[]}}"#,
        )
        .unwrap();
        fixture.send(ProfileRequested(other.path().to_path_buf()));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture.app.update();
            if fixture.mode("one").as_deref() == Some("other") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "replacement profile did not become ready"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut queued = fixture.app.world_mut().query::<&RememberMode>();
        assert_eq!(queued.iter(fixture.app.world()).count(), 1);
        drop(lock);
        fixture.send(ProfileRequested(fixture.directory.path().to_path_buf()));
        fixture.settle();
        assert_eq!(fixture.mode("one").as_deref(), Some("new"));
        assert_eq!(fixture.stored(Kind::Modes)["one"]["selected"], "new");
    }

    #[test]
    fn quitting_refuses_an_unavailable_lock_within_a_bounded_wait() {
        let modes = r#"{"one":{"selected":"old","modes":[]}}"#;
        let mut fixture = Fixture::new(None, Some(modes));
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(fixture.directory.path().join(".agent-modes.json.lock"))
            .unwrap();
        lock.lock().unwrap();
        fixture.send(RememberMode {
            agent_id: "one".into(),
            mode_id: "new".into(),
        });
        fixture.app.update();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(4));
            drop(lock);
        });
        fixture.app.world_mut().write_message(AppExit::Success);
        let started = Instant::now();
        fixture.app.update();
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "exit waited for the unbounded native lock"
        );
        assert_eq!(fixture.stored(Kind::Modes)["one"]["selected"], "old");
        release.join().unwrap();
    }

    #[test]
    fn an_inactive_profiles_locks_cannot_starve_the_current_profiles_exit_save() {
        let models = r#"{"one":{"selected":"old","models":[{"id":"old","name":"Old"},{"id":"new","name":"New"}]}}"#;
        let modes = r#"{"one":{"selected":"old","modes":[{"id":"old","name":"Old"},{"id":"new","name":"New"}]}}"#;
        let mut fixture = Fixture::new(Some(models), Some(modes));
        let mut locks = Vec::new();
        for kind in [Kind::Models, Kind::Modes] {
            let lock = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(
                    fixture
                        .directory
                        .path()
                        .join(format!(".{}.lock", kind.filename())),
                )
                .unwrap();
            lock.lock().unwrap();
            locks.push(lock);
        }
        fixture.send(RememberModel {
            agent_id: "one".into(),
            model_id: "new".into(),
        });
        fixture.send(RememberMode {
            agent_id: "one".into(),
            mode_id: "new".into(),
        });
        fixture.app.update();
        let other = tempfile::tempdir().unwrap();
        std::fs::write(other.path().join("agent-modes.json"), modes).unwrap();
        fixture.send(ProfileRequested(other.path().to_path_buf()));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture.app.update();
            if fixture.mode("one").as_deref() == Some("old") {
                break;
            }
            assert!(Instant::now() < deadline, "current profile did not load");
            std::thread::sleep(Duration::from_millis(1));
        }
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(5));
            drop(locks);
        });
        fixture.send(RememberMode {
            agent_id: "one".into(),
            mode_id: "new".into(),
        });
        fixture.app.world_mut().write_message(AppExit::Success);
        fixture.app.update();
        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(other.path().join("agent-modes.json")).unwrap())
                .unwrap();
        assert_eq!(stored["one"]["selected"], "new");
        release.join().unwrap();
    }

    #[test]
    fn malformed_mode_data_reports_failure_and_explicit_retry_replays_retained_input() {
        let mut fixture = Fixture::new(None, Some("{"));
        fixture.send(RememberMode {
            agent_id: "one".into(),
            mode_id: "new".into(),
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture.app.update();
            let failed = fixture
                .app
                .world_mut()
                .run_system_once(|preferences: AgentPreferences| {
                    preferences.load_error(Kind::Modes).is_some()
                })
                .unwrap();
            if failed {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "mode loading failure was not exposed"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            std::fs::read_to_string(fixture.directory.path().join("agent-modes.json")).unwrap(),
            "{"
        );
        std::fs::write(fixture.directory.path().join("agent-modes.json"), r#"{"one":{"selected":"old","modes":[{"id":"old","name":"Old"},{"id":"new","name":"New"}]},"two":{"selected":"kept","modes":[]}}"#).unwrap();
        fixture.send(RetryPreferences { kind: Kind::Modes });
        fixture.settle();
        assert_eq!(fixture.mode("one").as_deref(), Some("new"));
        assert_eq!(fixture.mode("two").as_deref(), Some("kept"));
        assert!(
            fixture
                .app
                .world_mut()
                .run_system_once(|preferences: AgentPreferences| preferences
                    .load_error(Kind::Modes)
                    .is_none())
                .unwrap()
        );
    }
}
