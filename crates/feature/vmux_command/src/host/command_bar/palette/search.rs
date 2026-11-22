use super::{OpenVersion, PaletteDraftInput, PaletteSnapshot};
use bevy::prelude::Commands;
use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use std::time::Duration;
use vmux_api::command_bar::{
    CommandBarUiState, CommandBarUiStatePatch, CommandPaletteDraftRequest,
    HistorySuggestionsRequest, HistorySuggestionsResponse, PathCompleteRequest,
    PathCompleteResponse,
};
use vmux_ecs::UiStateWrite;
use vmux_ecs::launcher::{HostsLauncher, RendersLauncherPanel};
use vmux_ecs::search::{Cancel, Ready, Request, SearchPlugin, State};

pub(super) struct PaletteSearchPlugin;

impl Plugin for PaletteSearchPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            SearchPlugin::<CompletionSearch>::new(PaletteSearch::COMPLETION_DEBOUNCE),
            SearchPlugin::<HistorySearch>::new(PaletteSearch::HISTORY_DEBOUNCE),
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

impl PaletteSearch {
    const COMPLETION_DEBOUNCE: Duration = Duration::from_millis(60);
    const HISTORY_DEBOUNCE: Duration = Duration::from_millis(300);
    const HISTORY_SUGGESTION_LIMIT: u32 = 5;

    fn parse_history_query(query: &str) -> Option<&str> {
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

type NewSearchPages<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        Or<(With<RendersLauncherPanel>, With<HostsLauncher>)>,
        Without<PaletteSearch>,
    ),
>;

fn attach(pages: NewSearchPages, mut commands: Commands) {
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
        snapshot.open_id = request.open_id;
        snapshot.completions.clear();
        snapshot.completions_partial = false;
        snapshot.completions_total = 0;
        snapshot.history.clear();
    }
    search.start = request.start;
    if !opened && search.query == request.query {
        return;
    }
    search.query.clone_from(&request.query);
    if let Some(query) = PaletteDraftInput::parse_completion_query(&search.query) {
        commands.trigger(Request::<CompletionSearch>::new(target, query));
    } else {
        commands.trigger(Cancel::<CompletionSearch>::new(target));
        snapshot.completions.clear();
        snapshot.completions_partial = false;
        snapshot.completions_total = 0;
    }
    snapshot.history.clear();
    if search.start {
        commands.trigger(Cancel::<HistorySearch>::new(target));
        return;
    }
    if let Some(query) = PaletteSearch::parse_history_query(&search.query) {
        commands.trigger(Request::<HistorySearch>::new(target, query.to_string()));
    } else {
        commands.trigger(Cancel::<HistorySearch>::new(target));
    }
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
    snapshot.completions.clone_from(&response.completions);
    snapshot.completions_partial = response.truncated;
    snapshot.completions_total = response.total;
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
    snapshot.history.clone_from(&response.entries);
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
            limit: PaletteSearch::HISTORY_SUGGESTION_LIMIT,
            request_id: ready.revision,
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::command_bar::{OpenId, PathEntry};

    #[test]
    fn history_only_queries_page_like_text() {
        assert_eq!(
            PaletteSearch::parse_history_query("rust docs"),
            Some("rust docs")
        );
        assert_eq!(
            PaletteSearch::parse_history_query("example.com"),
            Some("example.com")
        );
        assert_eq!(PaletteSearch::parse_history_query(""), None);
        assert_eq!(PaletteSearch::parse_history_query("> close"), None);
        assert_eq!(PaletteSearch::parse_history_query("/usr/bin"), None);
        assert_eq!(PaletteSearch::parse_history_query("~/notes"), None);
        assert_eq!(PaletteSearch::parse_history_query("vmux://settings/"), None);
        assert_eq!(PaletteSearch::parse_history_query("file:///tmp/a"), None);
    }

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
                .completions[0]
                .full_path,
            "/repo/src/main.rs"
        );
    }
}
