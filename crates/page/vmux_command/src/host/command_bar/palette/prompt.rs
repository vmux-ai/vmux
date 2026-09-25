use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::chat::{PromptHistory, PromptHistoryRequest};
use vmux_api::command_bar::{
    CommandBarUiState, CommandBarUiStatePatch, CommandPalettePromptHistoryRequest,
};
use vmux_core::host::UiStateWrite;
use vmux_core::launcher::{HostsLauncher, RendersLauncherPanel};

use super::{OpenVersion, PaletteSnapshot};

pub(super) struct PalettePromptPlugin;

impl Plugin for PalettePromptPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(request_palette_prompt_history)
            .add_observer(receive_palette_prompt_history)
            .add_systems(PreUpdate, attach_palette_prompt);
    }
}

#[derive(Component, Default)]
pub(super) struct PalettePrompt {
    open: OpenVersion,
    desired: Option<PromptContext>,
    loaded: Option<PromptContext>,
    inflight: Option<PromptFlight>,
}

fn attach_palette_prompt(
    pages: Query<
        Entity,
        (
            Or<(With<RendersLauncherPanel>, With<HostsLauncher>)>,
            Without<PalettePrompt>,
        ),
    >,
    mut commands: Commands,
) {
    for page in &pages {
        commands.entity(page).insert(PalettePrompt::default());
    }
}

fn request_palette_prompt_history(
    trigger: On<UiInput<CommandPalettePromptHistoryRequest>>,
    mut palettes: Query<(&mut PalettePrompt, &mut PaletteSnapshot)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let request = &trigger.event().payload;
    let Ok((mut prompt, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    let Some(opened) = prompt.open.accept(request.open_id) else {
        return;
    };
    if opened {
        prompt.desired = None;
        prompt.loaded = None;
        snapshot.0.open_id = request.open_id;
        snapshot.0.prompt_history.clear();
    }
    let desired = PromptContext::new(&request.agent, &request.cwd);
    if prompt.desired != desired {
        prompt.desired = desired;
        prompt.loaded = None;
        snapshot.0.prompt_history.clear();
    }
    request_history(target, &mut prompt, &mut commands);
}

fn receive_palette_prompt_history(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(&mut PalettePrompt, &mut PaletteSnapshot)>,
    mut commands: Commands,
) {
    let Some(response) = <CommandBarUiStatePatch as vmux_api::UiStatePatch<PromptHistory>>::payload(
        trigger.event().patch(),
    ) else {
        return;
    };
    let target = trigger.event().webview();
    let Ok((mut prompt, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    let Some(flight) = prompt.inflight.take() else {
        return;
    };
    if flight.open_generation == prompt.open.generation()
        && prompt.desired.as_ref() == Some(&flight.context)
    {
        snapshot.0.prompt_history.clone_from(&response.prompts);
        prompt.loaded = Some(flight.context);
    }
    request_history(target, &mut prompt, &mut commands);
}

fn request_history(target: Entity, prompt: &mut PalettePrompt, commands: &mut Commands) {
    if prompt.inflight.is_some() || prompt.desired == prompt.loaded {
        return;
    }
    let Some(context) = prompt.desired.clone() else {
        return;
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

#[derive(Clone, PartialEq, Eq)]
struct PromptContext {
    agent: String,
    cwd: String,
}

impl PromptContext {
    fn new(agent: &str, cwd: &str) -> Option<Self> {
        if agent.is_empty() || cwd.is_empty() {
            return None;
        }
        Some(Self {
            agent: agent.to_string(),
            cwd: cwd.to_string(),
        })
    }
}

struct PromptFlight {
    open_generation: u64,
    context: PromptContext,
}
