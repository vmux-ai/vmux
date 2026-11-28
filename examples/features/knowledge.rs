use std::path::PathBuf;

use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_app::knowledge::{
    KnowledgeGraph, KnowledgeGraphPlugin, KnowledgeIndex, LiveKnowledge, MaterializeKnowledge,
    RawSnapshot,
};
use vmux_app::prelude::*;

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
struct SearchHitRow {
    title: String,
    path: String,
}

#[ui_state(Default, Eq)]
struct KnowledgeUiState {
    query: String,
    backlink_count: usize,
    broken_links: Vec<String>,
    hits: Vec<SearchHitRow>,
}

#[ui_event(Eq)]
struct SearchKnowledgeEvent {
    query: String,
}

#[derive(Resource)]
struct KnowledgeDemoVault {
    _temp: tempfile::TempDir,
    root: PathBuf,
    guide_path: PathBuf,
    query: String,
    initial_snapshot: Option<RawSnapshot>,
}

struct KnowledgeDemoPlugin;

impl Plugin for KnowledgeDemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            UiStatePlugin::<KnowledgeUiState>::default(),
            UiIpcEventPlugin::<SearchKnowledgeEvent>::default(),
        ))
        .add_systems(Startup, materialize_vault)
        .add_observer(handle_search)
        .add_systems(Update, publish_knowledge);
    }
}

fn materialize_vault(mut demo: ResMut<KnowledgeDemoVault>, mut commands: Commands) {
    let owner = commands
        .spawn((KnowledgeIndex::default(), LiveKnowledge))
        .id();
    if let Some(snapshot) = demo.initial_snapshot.take() {
        commands.trigger(MaterializeKnowledge {
            entity: owner,
            snapshot,
        });
    }
}

fn handle_search(
    trigger: On<UiIpcInput<SearchKnowledgeEvent>>,
    mut demo: ResMut<KnowledgeDemoVault>,
) {
    demo.query = trigger.event().query.clone();
}

fn publish_knowledge(
    owners: Query<Entity, With<LiveKnowledge>>,
    demo: Res<KnowledgeDemoVault>,
    graph: KnowledgeGraph,
    pages: Query<Entity, (With<HostedPage>, With<UiIpcReady>)>,
    mut writer: UiStateWriter<KnowledgeUiState>,
) {
    let Some(owner) = owners.iter().next() else {
        return;
    };
    let resolved = graph.resolve(owner, &demo.guide_path, "Arch", Some("Overview"));
    let backlink_count = if resolved.exists {
        graph.backlinks(owner, &resolved.path).len()
    } else {
        0
    };
    let broken_links: Vec<String> = graph
        .broken_links(owner, &demo.guide_path)
        .into_iter()
        .map(|link| link.target)
        .collect();
    let query = if demo.query.trim().is_empty() {
        "Canonical ECS"
    } else {
        demo.query.trim()
    };
    let hits: Vec<SearchHitRow> = graph
        .search(owner, query, 10)
        .into_iter()
        .map(|hit| SearchHitRow {
            title: hit.title,
            path: hit
                .path
                .strip_prefix(&demo.root)
                .unwrap_or(&hit.path)
                .display()
                .to_string(),
        })
        .collect();
    for page in &pages {
        writer.push(
            page,
            KnowledgeUiState {
                query: demo.query.clone(),
                backlink_count,
                broken_links: broken_links.clone(),
                hits: hits.clone(),
            },
        );
    }
}

#[component]
fn KnowledgePageView() -> Element {
    let binding = use_ui_state::<KnowledgeUiState>();
    let state = binding.state.read().clone();
    rsx! {
        div { class: "root",
            h1 { "Vmux Knowledge Graph (vmux_knowledge)" }
            p { class: "meta", "Backlinks to Architecture: {state.backlink_count}" }
            input {
                value: "{state.query}",
                placeholder: "Search notes (default: Canonical ECS)",
                oninput: move |event| {
                    let _ = send_ui_event(&SearchKnowledgeEvent { query: event.value() });
                },
            }
            h2 { "Search Results" }
            ul {
                for hit in state.hits {
                    li { "{hit.title} — {hit.path}" }
                }
            }
            h2 { "Broken Wikilinks" }
            ul {
                for broken in state.broken_links {
                    li { "[[{broken}]]" }
                }
            }
        }
    }
}

static KNOWLEDGE_PAGE: Page = Page {
    permissions: &[
        KnowledgeUiState::PERMISSION,
        SearchKnowledgeEvent::PERMISSION,
        vmux_app::api::PageReady::PERMISSION,
    ],
    ..Page::pane("app://feature-knowledge/", KnowledgePageView).titled("Knowledge")
};

const CSS: &str = "body{background:#111318;color:#f3f4f6;font-family:system-ui,sans-serif;padding:24px}input{width:100%;padding:8px 12px;border-radius:8px;border:1px solid #334155;background:#1e293b;color:#fff;margin:12px 0;box-sizing:border-box}ul{list-style:none;padding:0}li{padding:6px 0;border-bottom:1px solid #1f2937}.meta{color:#94a3b8}";

fn main() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    std::fs::create_dir_all(root.join("notes")).unwrap();
    std::fs::write(
        root.join("notes/architecture.md"),
        "---\ntitle: ECS Architecture\naliases: [Arch]\n---\n\n# Overview\n\nCanonical ECS owns state.\n",
    )
    .unwrap();
    let guide_path = root.join("notes/guide.md");
    std::fs::write(
        &guide_path,
        "# Guide\n\nRead [[Arch#Overview|the architecture]] and [[Missing Page]].\n",
    )
    .unwrap();
    let initial_snapshot = Some(RawSnapshot::capture(&root).unwrap());

    let mut config = WindowHostConfig::new("Vmux Knowledge")
        .size(760, 520)
        .page(&KNOWLEDGE_PAGE)
        .stylesheet(CSS);
    if std::env::var_os("VMUX_EXAMPLE_HEADLESS").is_some() {
        config = config.headless();
    }

    App::new()
        .insert_resource(KnowledgeDemoVault {
            _temp: temp,
            root,
            guide_path,
            query: String::new(),
            initial_snapshot,
        })
        .add_plugins((
            DefaultPlugins.set(WindowHostPlugin::new(config)),
            KnowledgeGraphPlugin,
            KnowledgeDemoPlugin,
        ))
        .run();
}
