use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use dioxus::prelude::*;
use moonshine_save::prelude::*;
use vmux_app::ecs::time::BackoffPolicy;
use vmux_app::prelude::*;

#[derive(Resource)]
struct NotesBootstrap {
    _temp: Option<tempfile::TempDir>,
    store_path: PathBuf,
    initial_titles: Vec<String>,
}

#[ui_state(Default, Eq)]
struct NotesUiState {
    titles: Vec<String>,
}

#[ui_event(Eq)]
struct AddNoteRequest {
    title: String,
}

#[derive(Component, Clone, Default, Reflect)]
#[reflect(Component)]
struct Note;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect)]
#[reflect(Component)]
struct NoteId(String);

#[vmux_ecs::store(track; Note, NoteId, Name, Revision)]
struct NotesStore;

struct NotesPlugin;

impl Plugin for NotesPlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<Note, Save>()
            .add_plugins((
                IdentityIndexPlugin::<NoteId, Note>::default(),
                SchemaPlugin::<NotesStore>::default(),
                AutoSavePlugin::<NotesStore>::default(),
                UiStatePlugin::<NotesUiState>::default(),
                UiIpcEventPlugin::<AddNoteRequest>::default(),
            ))
            .add_systems(Startup, seed_notes)
            .add_observer(add_note)
            .add_observer(write_notes)
            .add_systems(Update, publish_notes);
    }
}

fn seed_notes(mut bootstrap: ResMut<NotesBootstrap>, mut commands: Commands) {
    let titles = std::mem::take(&mut bootstrap.initial_titles);
    for (index, title) in titles.into_iter().enumerate() {
        let id = format!("note-{}", index + 1);
        commands.spawn((Note, NoteId(id), Name::new(title), Revision(0)));
    }
    commands.spawn((
        AutoSave::<NotesStore>::default(),
        AutoSavePolicy::new(
            Duration::from_millis(500),
            Some(Duration::from_secs(2)),
            BackoffPolicy::exponential(Duration::from_secs(1), Duration::from_secs(8)),
        ),
        StorePath::new(bootstrap.store_path.clone()),
    ));
}

fn add_note(
    trigger: On<UiIpcInput<AddNoteRequest>>,
    existing: Query<(), With<Note>>,
    mut commands: Commands,
) {
    let title = trigger.event().title.trim();
    if title.is_empty() {
        return;
    }
    let id = format!("note-{}", existing.iter().count() + 1);
    commands.spawn((Note, NoteId(id), Name::new(title.to_string()), Revision(0)));
}

fn write_notes(
    request: On<StoreWriteRequest<NotesStore>>,
    notes: Query<&Name, With<Note>>,
    mut commands: Commands,
) {
    let mut titles: Vec<&str> = notes.iter().map(Name::as_str).collect();
    titles.sort();
    let payload = titles.join("\n");
    let result = if let Some(parent) = request.path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        StoreWriteResult::<NotesStore>::err(request.entity, request.revision, error.to_string())
    } else {
        match std::fs::write(&request.path, payload) {
            Ok(()) => StoreWriteResult::<NotesStore>::ok(request.entity, request.revision),
            Err(error) => StoreWriteResult::<NotesStore>::err(
                request.entity,
                request.revision,
                error.to_string(),
            ),
        }
    };
    commands.trigger(result);
}

fn load_saved_titles(path: &Path) -> Vec<String> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn default_notes_path() -> PathBuf {
    if let Some(path) = std::env::var_os("VMUX_NOTES_PATH") {
        return PathBuf::from(path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".vmux/examples/notes.txt");
    }
    std::env::temp_dir().join("vmux-examples-notes.txt")
}

type ChangedNotes<'w, 's> = Query<'w, 's, (), (With<Note>, Or<(Added<Note>, Changed<Name>)>)>;

fn publish_notes(
    notes: Query<&Name, With<Note>>,
    changed: ChangedNotes,
    pages: Query<Entity, (With<HostedPage>, With<UiIpcReady>)>,
    new_pages: Query<(), (With<HostedPage>, Added<UiIpcReady>)>,
    mut writer: UiStateWriter<NotesUiState>,
) {
    if changed.is_empty() && new_pages.is_empty() {
        return;
    }
    let mut titles: Vec<String> = notes.iter().map(|name| name.as_str().to_string()).collect();
    titles.sort();
    for page in &pages {
        writer.push(
            page,
            NotesUiState {
                titles: titles.clone(),
            },
        );
    }
}

#[component]
fn NotesPageView() -> Element {
    let binding = use_ui_state::<NotesUiState>();
    let titles = binding.state.read().titles.clone();
    let mut draft = use_signal(String::new);
    rsx! {
        div { class: "notes",
            h1 { "Vmux Notes" }
            div { class: "row",
                input {
                    value: "{draft}",
                    placeholder: "New note title",
                    oninput: move |event| draft.set(event.value()),
                }
                button {
                    onclick: move |_| {
                        let title = draft();
                        if !title.trim().is_empty() {
                            let _ = send_ui_event(&AddNoteRequest { title });
                            draft.set(String::new());
                        }
                    },
                    "Add"
                }
            }
            ul {
                for title in titles {
                    li { "{title}" }
                }
            }
        }
    }
}

static NOTES_PAGE: Page = Page {
    permissions: &[
        NotesUiState::PERMISSION,
        AddNoteRequest::PERMISSION,
        vmux_app::api::PageReady::PERMISSION,
    ],
    ..Page::pane("app://notes/", NotesPageView).titled("Notes")
};

fn main() {
    let headless = std::env::var_os("VMUX_EXAMPLE_HEADLESS").is_some();
    let temp_dir = headless.then(|| tempfile::tempdir().unwrap());
    let store_path = match &temp_dir {
        Some(dir) => dir.path().join("notes.txt"),
        None => default_notes_path(),
    };
    let initial_titles = load_saved_titles(&store_path);
    let mut config = WindowHostConfig::new("Vmux Notes")
        .size(640, 480)
        .page(&NOTES_PAGE);
    if headless {
        config = config.headless();
    }
    App::new()
        .insert_resource(NotesBootstrap {
            _temp: temp_dir,
            store_path,
            initial_titles,
        })
        .add_plugins((
            bevy_app::TaskPoolPlugin::default(),
            DefaultPlugins.set(WindowHostPlugin::new(config)),
            NotesPlugin,
        ))
        .run();
}
