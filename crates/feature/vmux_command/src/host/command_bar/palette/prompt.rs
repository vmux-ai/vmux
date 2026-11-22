use super::{OpenVersion, PaletteContext, PaletteSnapshot};
use bevy::prelude::*;
use bevy::prelude::{Changed, Commands};
use bevy_cef::prelude::UiInput;
use vmux_api::chat::{PromptHistory, PromptHistoryRequest};
use vmux_api::command_bar::{CommandBarUiState, CommandBarUiStatePatch};
use vmux_ecs::UiStateWrite;
use vmux_ecs::launcher::{HostsLauncher, RendersLauncherPanel};

pub(super) struct PalettePromptPlugin;

impl Plugin for PalettePromptPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(receive_history)
            .add_systems(PreUpdate, attach)
            .add_systems(
                PostUpdate,
                (update, request_history).chain().before(super::project),
            );
    }
}

#[derive(Component, Default)]
pub(super) struct PalettePrompt {
    open: OpenVersion,
    desired: Option<PromptContext>,
    loaded: Option<PromptContext>,
    inflight: Option<PromptFlight>,
}

impl PalettePrompt {
    fn context_for(agent: &str, cwd: &str) -> Option<PromptContext> {
        if agent.is_empty() || cwd.is_empty() {
            return None;
        }
        Some(PromptContext {
            agent: agent.to_string(),
            cwd: cwd.to_string(),
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
struct PromptContext {
    agent: String,
    cwd: String,
}

#[derive(Clone, PartialEq, Eq)]
struct PromptFlight {
    open_generation: u64,
    context: PromptContext,
}

type NewPromptPages<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        Or<(With<RendersLauncherPanel>, With<HostsLauncher>)>,
        Without<PalettePrompt>,
    ),
>;

fn attach(pages: NewPromptPages, mut commands: Commands) {
    for page in &pages {
        commands.entity(page).insert(PalettePrompt::default());
    }
}

fn update(
    mut palettes: Query<
        (
            Entity,
            &PaletteContext,
            &mut PalettePrompt,
            &mut PaletteSnapshot,
        ),
        Changed<PaletteContext>,
    >,
) {
    for (_, context, mut prompt, mut snapshot) in &mut palettes {
        let Some(opened) = prompt.open.accept(context.open_id) else {
            continue;
        };
        if opened {
            prompt.desired = None;
            prompt.loaded = None;
            snapshot.open_id = context.open_id;
            snapshot.prompt_history.clear();
        }
        let desired = PalettePrompt::context_for(&context.agent, &context.cwd);
        if prompt.desired != desired {
            prompt.desired = desired;
            prompt.loaded = None;
            snapshot.prompt_history.clear();
        }
    }
}

fn receive_history(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(&mut PalettePrompt, &mut PaletteSnapshot)>,
) {
    let Some(response) = <CommandBarUiStatePatch as vmux_api::UiStatePatch<PromptHistory>>::payload(
        trigger.event().update(),
    ) else {
        return;
    };
    let Ok((mut prompt, mut snapshot)) = palettes.get_mut(trigger.event().webview()) else {
        return;
    };
    let Some(flight) = prompt.inflight.take() else {
        return;
    };
    if flight.open_generation == prompt.open.generation()
        && prompt.desired.as_ref() == Some(&flight.context)
    {
        snapshot.prompt_history.clone_from(&response.prompts);
        prompt.loaded = Some(flight.context);
    }
}

fn request_history(mut palettes: Query<(Entity, &mut PalettePrompt)>, mut commands: Commands) {
    for (target, mut prompt) in &mut palettes {
        if prompt.inflight.is_some() || prompt.desired == prompt.loaded {
            continue;
        }
        let Some(context) = prompt.desired.clone() else {
            continue;
        };
        prompt.inflight = Some(PromptFlight {
            open_generation: prompt.open.generation(),
            context: context.clone(),
        });
        commands.trigger(UiInput {
            webview: target,
            payload: PromptHistoryRequest {
                agent: context.agent,
                cwd: context.cwd,
            },
        });
    }
}
