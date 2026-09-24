use std::time::Duration;

use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::command_bar::{
    CommandBarUiState, CommandBarUiStatePatch, CommandPaletteDraftRequest,
    HistorySuggestionsRequest, HistorySuggestionsResponse, PathCompleteRequest,
    PathCompleteResponse,
};
use vmux_core::host::UiStateWrite;
use vmux_core::launcher::{HostsLauncher, RendersLauncherPanel};
use vmux_ui::launcher::palette::CompletionQuery;

use super::{OpenVersion, PaletteSnapshot, PendingPaletteRequest, RequestDelay, RequestGeneration};

const COMPLETION_DEBOUNCE: Duration = Duration::from_millis(60);
const HISTORY_DEBOUNCE: Duration = Duration::from_millis(300);
const HISTORY_SUGGESTION_LIMIT: u32 = 5;

pub(super) struct PaletteSearchPlugin;

impl Plugin for PaletteSearchPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(update_palette_search_draft)
            .add_observer(receive_palette_completion)
            .add_observer(receive_palette_history)
            .add_systems(PreUpdate, attach_palette_search)
            .add_systems(
                Update,
                (dispatch_completion_request, dispatch_history_request),
            );
    }
}

#[derive(Component, Default)]
pub(super) struct PaletteSearch {
    open: OpenVersion,
    start: bool,
    query: String,
    completion_generation: RequestGeneration,
    history_generation: RequestGeneration,
}

fn attach_palette_search(
    pages: Query<
        Entity,
        (
            Or<(With<RendersLauncherPanel>, With<HostsLauncher>)>,
            Without<PaletteSearch>,
        ),
    >,
    mut commands: Commands,
) {
    for page in &pages {
        commands.entity(page).insert(PaletteSearch::default());
    }
}

fn update_palette_search_draft(
    trigger: On<UiInput<CommandPaletteDraftRequest>>,
    mut palettes: Query<(&mut PaletteSearch, &mut PaletteSnapshot)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let request = &trigger.event().payload;
    let Ok((mut search, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    let Some(opened) = search.open.accept(request.open_id) else {
        return;
    };
    if opened {
        search.query.clear();
        search.completion_generation.advance();
        search.history_generation.advance();
        snapshot.0.open_id = request.open_id;
        snapshot.0.completions.clear();
        snapshot.0.completions_partial = false;
        snapshot.0.completions_total = 0;
        snapshot.0.history.clear();
    }
    search.start = request.start;
    if !opened && search.query == request.query {
        return;
    }
    search.query.clone_from(&request.query);
    search.complete(target, &mut snapshot, &mut commands);
    search.suggest(target, &mut snapshot, &mut commands);
}

fn receive_palette_completion(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(&PaletteSearch, &mut PaletteSnapshot)>,
) {
    let Some(response) =
        <CommandBarUiStatePatch as vmux_api::UiStatePatch<PathCompleteResponse>>::payload(
            trigger.event().patch(),
        )
    else {
        return;
    };
    let Ok((search, mut snapshot)) = palettes.get_mut(trigger.event().webview()) else {
        return;
    };
    if !search.completion_generation.matches(response.request_id) {
        return;
    }
    snapshot.0.completions.clone_from(&response.completions);
    snapshot.0.completions_partial = response.truncated;
    snapshot.0.completions_total = response.total;
}

fn receive_palette_history(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(&PaletteSearch, &mut PaletteSnapshot)>,
) {
    let Some(response) = <CommandBarUiStatePatch as vmux_api::UiStatePatch<
        HistorySuggestionsResponse,
    >>::payload(trigger.event().patch()) else {
        return;
    };
    let Ok((search, mut snapshot)) = palettes.get_mut(trigger.event().webview()) else {
        return;
    };
    if !search.history_generation.matches(response.request_id) {
        return;
    }
    snapshot.0.history.clone_from(&response.entries);
}

impl PaletteSearch {
    fn complete(
        &mut self,
        target: Entity,
        snapshot: &mut PaletteSnapshot,
        commands: &mut Commands,
    ) {
        let generation = self.completion_generation.advance();
        snapshot.0.completions.clear();
        snapshot.0.completions_partial = false;
        snapshot.0.completions_total = 0;
        let Some(query) = CompletionQuery::parse(&self.query) else {
            return;
        };
        CompletionRequestDelay::spawn(target, generation, query, commands);
    }

    fn suggest(&mut self, target: Entity, snapshot: &mut PaletteSnapshot, commands: &mut Commands) {
        let generation = self.history_generation.advance();
        snapshot.0.history.clear();
        if self.start {
            return;
        }
        let Some(query) = HistoryQuery::parse(&self.query) else {
            return;
        };
        HistoryRequestDelay::spawn(target, generation, query.to_string(), commands);
    }
}

#[derive(Component)]
struct CompletionRequestDelay(RequestDelay);

impl CompletionRequestDelay {
    fn spawn(target: Entity, generation: u64, query: String, commands: &mut Commands) {
        commands.spawn((
            Name::new("Command Palette Completion Debounce"),
            Self(RequestDelay::new(
                target,
                generation,
                query,
                COMPLETION_DEBOUNCE,
            )),
            PendingPaletteRequest,
        ));
    }
}

fn dispatch_completion_request(
    delays: Query<(Entity, &CompletionRequestDelay)>,
    searches: Query<&PaletteSearch>,
    mut commands: Commands,
) {
    for (entity, delay) in &delays {
        if !delay.0.ready() {
            continue;
        }
        commands.entity(entity).despawn();
        let Ok(search) = searches.get(delay.0.target) else {
            continue;
        };
        if !search.completion_generation.matches(delay.0.generation) {
            continue;
        }
        commands.trigger(UiInput {
            webview: delay.0.target,
            payload: PathCompleteRequest {
                request_id: delay.0.generation,
                query: delay.0.query.clone(),
            },
        });
    }
}

#[derive(Component)]
struct HistoryRequestDelay(RequestDelay);

impl HistoryRequestDelay {
    fn spawn(target: Entity, generation: u64, query: String, commands: &mut Commands) {
        commands.spawn((
            Name::new("Command Palette History Debounce"),
            Self(RequestDelay::new(
                target,
                generation,
                query,
                HISTORY_DEBOUNCE,
            )),
            PendingPaletteRequest,
        ));
    }
}

fn dispatch_history_request(
    delays: Query<(Entity, &HistoryRequestDelay)>,
    searches: Query<&PaletteSearch>,
    mut commands: Commands,
) {
    for (entity, delay) in &delays {
        if !delay.0.ready() {
            continue;
        }
        commands.entity(entity).despawn();
        let Ok(search) = searches.get(delay.0.target) else {
            continue;
        };
        if !search.history_generation.matches(delay.0.generation) {
            continue;
        }
        commands.trigger(UiInput {
            webview: delay.0.target,
            payload: HistorySuggestionsRequest {
                query: delay.0.query.clone(),
                limit: HISTORY_SUGGESTION_LIMIT,
                request_id: delay.0.generation,
            },
        });
    }
}

struct HistoryQuery;

impl HistoryQuery {
    fn parse(query: &str) -> Option<&str> {
        let trimmed = query.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('>')
            || trimmed.starts_with('/')
            || trimmed.starts_with('~')
            || trimmed.starts_with("vmux://")
            || trimmed.starts_with("file:")
        {
            return None;
        }
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_only_queries_page_like_text() {
        assert_eq!(HistoryQuery::parse("rust docs"), Some("rust docs"));
        assert_eq!(HistoryQuery::parse("example.com"), Some("example.com"));
        assert_eq!(HistoryQuery::parse(""), None);
        assert_eq!(HistoryQuery::parse("> close"), None);
        assert_eq!(HistoryQuery::parse("/usr/bin"), None);
        assert_eq!(HistoryQuery::parse("~/notes"), None);
        assert_eq!(HistoryQuery::parse("vmux://settings/"), None);
        assert_eq!(HistoryQuery::parse("file:///tmp/a"), None);
    }
}
