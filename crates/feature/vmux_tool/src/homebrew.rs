use std::path::PathBuf;

use crate::state::ToolImportRequest;
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;

use vmux_ecs::OperationState;

use crate::manifest::{ToolStore, ToolsManifest};
use crate::{
    ToolOperationFailed, ToolOperationFinished, ToolOperationRequest, ToolOperationRouteSet,
    ToolOperationSucceeded, ToolProviderBinding, ToolProviderId, ToolProviderTarget,
    ToolStoreOperation, ToolStoreTarget, ToolTaskSet,
};

pub(crate) struct HomebrewToolPlugin;

impl Plugin for HomebrewToolPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(host)]
        app.add_plugins(crate::homebrew_inventory::HomebrewInventoryPlugin);
        app.add_systems(Startup, spawn)
            .add_systems(Update, route.in_set(ToolOperationRouteSet))
            .add_systems(
                Update,
                (import, read, commit, save)
                    .chain()
                    .in_set(ToolTaskSet::Start),
            )
            .add_systems(Update, drain.after(ToolTaskSet::Apply))
            .add_systems(Update, finish.after(ToolTaskSet::Apply));
    }
}

fn spawn(mut commands: Commands) {
    let formula = commands
        .spawn((ToolProviderBinding::new::<crate::Feature>(0),))
        .id();
    let cask = commands
        .spawn((ToolProviderBinding::new::<crate::Feature>(1),))
        .id();
    #[cfg(host)]
    {
        commands.entity(formula).insert((
            crate::observation::Handler,
            crate::observation::PackageProvider,
        ));
        commands.entity(cask).insert((
            crate::observation::Handler,
            crate::observation::PackageProvider,
        ));
    }
    #[cfg(not(host))]
    let _ = (formula, cask);
}

fn route(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolImportRequest>,
            &ToolProviderTarget,
        ),
        Added<ToolStoreTarget>,
    >,
    providers: Query<&ToolProviderId>,
    mut commands: Commands,
) {
    for (entity, operation, provider) in &requests {
        if !providers
            .get(provider.entity())
            .is_ok_and(|id| matches!(id.0.id(), "homebrew-formula" | "homebrew-cask"))
        {
            continue;
        }
        let request = &operation.0;
        let path = request.value.trim();
        if path.is_empty() {
            continue;
        }
        commands
            .entity(entity)
            .insert((ToolStoreOperation, ImportBrewfile::new(path)));
    }
}

fn finish(
    operations: Query<
        (Entity, &ImportedBrewfile),
        (With<ToolStoreOperation>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &operations {
        commands.entity(entity).insert((
            ToolOperationFinished,
            ToolOperationSucceeded(format!(
                "imported {} formulae and {} casks",
                output.formulae, output.casks
            )),
        ));
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ImportBrewfile {
    path: PathBuf,
}

impl ImportBrewfile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ImportedBrewfile {
    pub(super) formulae: usize,
    pub(super) casks: usize,
}

#[derive(Component)]
struct BrewPlan {
    formulae: usize,
    casks: usize,
}

#[derive(Component)]
struct Importing;

fn import(
    operations: Query<
        (
            Entity,
            &ImportBrewfile,
            &vmux_ecs::EntityTarget<ToolStore>,
            Option<&OperationState>,
        ),
        (Without<Importing>, Without<ToolOperationFinished>),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, _, target, state) in &operations {
        if state.is_some_and(|state| {
            !matches!(state, OperationState::Pending | OperationState::Running)
        }) {
            continue;
        }
        let Ok(store) = stores.get(target.entity()) else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".into()),
            ));
            continue;
        };
        commands.entity(entity).insert((
            Importing,
            store.clone(),
            vmux_ecs::OperationRetained,
            crate::storage::ReadRequest { retain_lease: true },
        ));
    }
}

fn read(
    operations: Query<
        (
            Entity,
            &ImportBrewfile,
            &ToolStore,
            &crate::storage::ReadResult,
        ),
        (With<Importing>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, input, store, result) in &operations {
        commands
            .entity(entity)
            .remove::<crate::storage::ReadResult>();
        let manifest = match &result.0 {
            Ok(manifest) => manifest.clone(),
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFinished, ToolOperationFailed(error.clone())));
                continue;
            }
        };
        let path = match store.expand_user_path(&input.path) {
            Ok(path) => path,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFinished, ToolOperationFailed(error)));
                continue;
            }
        };
        commands
            .entity(entity)
            .insert((manifest, crate::storage::FileReadRequest(path)));
    }
}

fn commit(
    mut operations: Query<
        (
            Entity,
            &ImportBrewfile,
            &crate::storage::FileReadResult,
            &mut ToolsManifest,
        ),
        (With<Importing>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, input, source, mut manifest) in &mut operations {
        commands
            .entity(entity)
            .remove::<crate::storage::FileReadResult>();
        let source = match &source.0 {
            Ok(source) => source,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFinished, ToolOperationFailed(error.clone())));
                continue;
            }
        };
        let document = BrewfileImport::parse(source);
        if document.formulae.is_empty() && document.casks.is_empty() {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed(format!(
                    "no formulae or casks found in {}",
                    input.path.display()
                )),
            ));
            continue;
        }
        let formulae = manifest.add_packages("homebrew-formula", &document.formulae);
        let casks = manifest.add_packages("homebrew-cask", &document.casks);
        commands.entity(entity).insert((
            BrewPlan { formulae, casks },
            crate::storage::BrewfileTemplate(source.clone()),
            crate::storage::WriteRequest {
                manifest: manifest.clone(),
                retain_lease: true,
            },
        ));
    }
}

fn save(
    operations: Query<
        (Entity, &BrewPlan, &crate::storage::WriteResult),
        (With<Importing>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, plan, result) in &operations {
        commands
            .entity(entity)
            .remove::<crate::storage::WriteResult>();
        match &result.0 {
            Ok(()) => {
                commands.entity(entity).insert(ImportedBrewfile {
                    formulae: plan.formulae,
                    casks: plan.casks,
                });
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFinished, ToolOperationFailed(error.clone())));
            }
        }
        commands.entity(entity).remove::<(
            Importing,
            BrewPlan,
            ToolStore,
            ToolsManifest,
            crate::storage::StoreLease,
            vmux_ecs::OperationRetained,
        )>();
    }
}

fn drain(
    operations: Query<
        Entity,
        (
            With<Importing>,
            With<ToolOperationFinished>,
            Without<crate::storage::ReadRequest>,
            Without<crate::storage::WriteRequest>,
            Without<crate::storage::FileReadRequest>,
        ),
    >,
    mut commands: Commands,
) {
    for entity in &operations {
        commands.entity(entity).remove::<(
            Importing,
            BrewPlan,
            ToolStore,
            ToolsManifest,
            crate::storage::StoreLease,
            vmux_ecs::OperationRetained,
            crate::storage::ReadResult,
            crate::storage::WriteResult,
            crate::storage::FileReadResult,
        )>();
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrewfileImport {
    pub formulae: Vec<String>,
    pub casks: Vec<String>,
}

impl BrewfileImport {
    pub fn parse(source: &str) -> Self {
        let mut import = Self::default();
        for line in source.lines() {
            if let Some(name) = Self::quoted_call(line, "brew") {
                import.formulae.push(name);
            } else if let Some(name) = Self::quoted_call(line, "cask") {
                import.casks.push(name);
            }
        }
        ToolsManifest::normalize_names(&mut import.formulae);
        ToolsManifest::normalize_names(&mut import.casks);
        import
    }

    fn quoted_call(line: &str, call: &str) -> Option<String> {
        let line = line.trim_start();
        let rest = line.strip_prefix(call)?;
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }
        let rest = rest.trim_start();
        let quote = rest.chars().next()?;
        if !matches!(quote, '\'' | '"') {
            return None;
        }
        let mut escaped = false;
        let mut name = String::new();
        for character in rest[quote.len_utf8()..].chars() {
            if escaped {
                name.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                return (!name.is_empty()).then_some(name);
            } else {
                name.push(character);
            }
        }
        None
    }
}
