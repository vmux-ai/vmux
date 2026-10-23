use std::time::Duration;

use super::super::driver::CompletionQuery;
use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::command_bar::{
    CommandBarUiState, CommandBarUiStatePatch, CommandPaletteDraftRequest,
    HistorySuggestionsRequest, HistorySuggestionsResponse, PathCompleteRequest,
    PathCompleteResponse,
};
use vmux_ecs::UiStateWrite;
use vmux_ecs::search::{Cancel, Ready, Request, SearchPlugin, State};

use super::search_driver::HistoryQuery;
use super::{NewPalette, OpenVersion, PaletteSnapshot};

const COMPLETION_DEBOUNCE: Duration = Duration::from_millis(60);
const HISTORY_DEBOUNCE: Duration = Duration::from_millis(300);
const HISTORY_SUGGESTION_LIMIT: u32 = 5;

pub(super) struct PaletteSearchPlugin;

impl Plugin for PaletteSearchPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            SearchPlugin::<CompletionSearch>::new(COMPLETION_DEBOUNCE),
            SearchPlugin::<HistorySearch>::new(HISTORY_DEBOUNCE),
        ))
        .add_observer(update)
        .add_observer(receive_files)
        .add_observer(receive_history)
        .add_observer(request_files)
        .add_observer(request_history)
        .add_systems(PreUpdate, attach);
    }
}

pub(super) struct CompletionSearch;

pub(super) struct HistorySearch;

#[derive(Component, Default)]
pub(super) struct PaletteSearch {
    open: OpenVersion,
    start: bool,
    query: String,
}

fn attach(pages: Query<Entity, NewPalette<PaletteSearch>>, mut commands: Commands) {
    for page in &pages {
        commands.entity(page).insert((
            PaletteSearch::default(),
            State::<CompletionSearch>::default(),
            State::<HistorySearch>::default(),
        ));
    }
}

fn update(
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
        commands.trigger(Cancel::<CompletionSearch>::new(target));
        commands.trigger(Cancel::<HistorySearch>::new(target));
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
    queue_files(target, &mut search, &mut snapshot, &mut commands);
    queue_history(target, &mut search, &mut snapshot, &mut commands);
}

fn receive_files(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(&State<CompletionSearch>, &mut PaletteSnapshot)>,
) {
    let Some(response) =
        <CommandBarUiStatePatch as vmux_api::UiStatePatch<PathCompleteResponse>>::payload(
            trigger.event().update(),
        )
    else {
        return;
    };
    let Ok((search, mut snapshot)) = palettes.get_mut(trigger.event().webview()) else {
        return;
    };
    if !search.matches(response.request_id) {
        return;
    }
    snapshot.0.completions.clone_from(&response.completions);
    snapshot.0.completions_partial = response.truncated;
    snapshot.0.completions_total = response.total;
}

fn receive_history(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(&State<HistorySearch>, &mut PaletteSnapshot)>,
) {
    let Some(response) = <CommandBarUiStatePatch as vmux_api::UiStatePatch<
        HistorySuggestionsResponse,
    >>::payload(trigger.event().update()) else {
        return;
    };
    let Ok((search, mut snapshot)) = palettes.get_mut(trigger.event().webview()) else {
        return;
    };
    if !search.matches(response.request_id) {
        return;
    }
    snapshot.0.history.clone_from(&response.entries);
}

fn queue_files(
    target: Entity,
    search: &PaletteSearch,
    snapshot: &mut PaletteSnapshot,
    commands: &mut Commands,
) {
    let Some(query) = CompletionQuery::parse(&search.query) else {
        commands.trigger(Cancel::<CompletionSearch>::new(target));
        snapshot.0.completions.clear();
        snapshot.0.completions_partial = false;
        snapshot.0.completions_total = 0;
        return;
    };
    commands.trigger(Request::<CompletionSearch>::new(target, query));
}

fn queue_history(
    target: Entity,
    search: &PaletteSearch,
    snapshot: &mut PaletteSnapshot,
    commands: &mut Commands,
) {
    snapshot.0.history.clear();
    if search.start {
        commands.trigger(Cancel::<HistorySearch>::new(target));
        return;
    }
    let Some(query) = HistoryQuery::parse(&search.query) else {
        commands.trigger(Cancel::<HistorySearch>::new(target));
        return;
    };
    commands.trigger(Request::<HistorySearch>::new(target, query.to_string()));
}

fn request_files(trigger: On<Ready<CompletionSearch>>, mut commands: Commands) {
    let ready = trigger.event();
    commands.trigger(UiInput {
        webview: ready.target,
        payload: PathCompleteRequest {
            request_id: ready.revision,
            query: ready.query.clone(),
        },
    });
}

fn request_history(trigger: On<Ready<HistorySearch>>, mut commands: Commands) {
    let ready = trigger.event();
    commands.trigger(UiInput {
        webview: ready.target,
        payload: HistorySuggestionsRequest {
            query: ready.query.clone(),
            limit: HISTORY_SUGGESTION_LIMIT,
            request_id: ready.revision,
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::command_bar::{OpenId, PathEntry};
    use vmux_ecs::launcher::RendersLauncherPanel;

    #[test]
    fn typing_keeps_file_rows_until_the_replacement_arrives() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PaletteSearchPlugin);
        let page = app
            .world_mut()
            .spawn((RendersLauncherPanel, PaletteSnapshot::default()))
            .id();
        app.update();

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: CommandPaletteDraftRequest {
                open_id: OpenId(1),
                query: "@main".to_string(),
                start: false,
            },
        });
        app.world_mut()
            .get_mut::<PaletteSnapshot>(page)
            .unwrap()
            .0
            .completions = vec![PathEntry {
            name: "main.rs".to_string(),
            full_path: "/repo/src/main.rs".to_string(),
            ..Default::default()
        }];

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: CommandPaletteDraftRequest {
                open_id: OpenId(1),
                query: "@main.r".to_string(),
                start: false,
            },
        });

        assert_eq!(
            app.world()
                .get::<PaletteSnapshot>(page)
                .unwrap()
                .0
                .completions[0]
                .full_path,
            "/repo/src/main.rs"
        );
    }
}
