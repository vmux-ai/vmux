use bevy::{
    asset::io::embedded::EmbeddedAssetRegistry,
    ecs::system::SystemParam,
    prelude::{
        App, Commands, Component, Entity, IntoScheduleConfigs, Message, MessageReader,
        MessageWriter, On, Plugin, Query, ResMut, Startup, SystemSet, Update, With,
    },
};
use bevy_cef::prelude::BinReceive;
use bevy_cef_core::prelude::CefEmbeddedHost;
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub struct PagePlugin;

impl Plugin for PagePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Startup, PageEmbedSet)
            .add_systems(Startup, embed_page_static_assets.in_set(PageEmbedSet));
    }
}

pub const PAGE_READY_BIN_EVENT_ID: &str = "vmux-page-ready";

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageManifest {
    pub host: &'static str,
    pub title: &'static str,
    pub title_message_id: Option<&'static str>,
    pub replaces_command: Option<&'static str>,
    pub keywords: &'static [&'static str],
    pub icon: Option<crate::icon::BuiltinIcon>,
    pub command_bar: bool,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostsPage;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindsEditingChords;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrewarmPage {
    pub host: &'static str,
    pub url: &'static str,
    pub title: &'static str,
    pub pool_size: usize,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativelyHosted {
    pub url: &'static str,
    pub title: &'static str,
}

pub(crate) struct HostHistoryPlugin;

impl Plugin for HostHistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<HostHistoryStep>()
            .add_message::<HostHistoryTraversed>()
            .configure_sets(
                Update,
                (
                    HostHistorySet::Step,
                    HostHistorySet::Apply,
                    HostHistorySet::Record,
                )
                    .chain(),
            )
            .add_systems(Update, step_host_history.in_set(HostHistorySet::Step));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HostHistorySet {
    Step,
    Apply,
    Record,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostHistoryDelta {
    Back,
    Forward,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostHistoryEntry {
    pub url: String,
    pub top_line: u32,
}

#[derive(Component, Clone, Debug, Default)]
pub struct HostHistory {
    entries: Vec<HostHistoryEntry>,
    cursor: usize,
}

impl HostHistory {
    pub const CAPACITY: usize = 50;

    pub fn can_go_back(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }

    pub fn showing(&self, url: &str, top_line: u32) -> bool {
        let Some(current) = self.entries.get(self.cursor) else {
            return false;
        };
        current.url == url && current.top_line == top_line
    }

    pub fn observe(&mut self, url: &str, top_line: u32) {
        if let Some(current) = self.entries.get_mut(self.cursor)
            && current.url == url
        {
            current.top_line = top_line;
            return;
        }
        self.entries.truncate(self.cursor + 1);
        self.entries.push(HostHistoryEntry {
            url: url.to_string(),
            top_line,
        });
        self.cursor = self.entries.len() - 1;
        let overflow = self.entries.len().saturating_sub(Self::CAPACITY);
        if overflow == 0 {
            return;
        }
        self.entries.drain(..overflow);
        self.cursor -= overflow;
    }

    fn step(&mut self, delta: HostHistoryDelta) -> Option<HostHistoryEntry> {
        match delta {
            HostHistoryDelta::Back => {
                if !self.can_go_back() {
                    return None;
                }
                self.cursor -= 1;
            }
            HostHistoryDelta::Forward => {
                if !self.can_go_forward() {
                    return None;
                }
                self.cursor += 1;
            }
        }
        self.entries.get(self.cursor).cloned()
    }
}

#[derive(Message, Clone, Copy, Debug)]
pub struct HostHistoryStep {
    pub webview: Entity,
    pub delta: HostHistoryDelta,
}

#[derive(Message, Clone, Debug)]
pub struct HostHistoryTraversed {
    pub webview: Entity,
    pub entry: HostHistoryEntry,
}

#[derive(SystemParam)]
pub struct HostHistoryNavigation<'w, 's> {
    hosted: Query<'w, 's, (), With<HostHistory>>,
    steps: MessageWriter<'w, HostHistoryStep>,
}

impl HostHistoryNavigation<'_, '_> {
    pub fn stepped(&mut self, webview: Entity, delta: HostHistoryDelta) -> bool {
        if !self.hosted.contains(webview) {
            return false;
        }
        self.steps.write(HostHistoryStep { webview, delta });
        true
    }
}

fn step_host_history(
    mut steps: MessageReader<HostHistoryStep>,
    mut histories: Query<&mut HostHistory>,
    mut traversed: MessageWriter<HostHistoryTraversed>,
) {
    for step in steps.read() {
        let Ok(mut history) = histories.get_mut(step.webview) else {
            continue;
        };
        let Some(entry) = history.step(step.delta) else {
            continue;
        };
        traversed.write(HostHistoryTraversed {
            webview: step.webview,
            entry,
        });
    }
}

impl PageManifest {
    pub fn embedded_host(&self) -> CefEmbeddedHost {
        CefEmbeddedHost {
            host: self.host.to_string(),
            default_document: embedded_default_document(self.host, "index.html"),
        }
    }

    pub fn url(&self) -> String {
        let host = self.host.trim().trim_matches('/');
        format!("vmux://{host}/")
    }

    fn bundle_root(&self, resources_dir: Option<&Path>) -> PathBuf {
        packaged_page_root(resources_dir, self.host)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vmux_ui/dist"))
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageEmbedSet;

#[derive(
    Clone,
    Copy,
    Component,
    Debug,
    Default,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PageReady {}

pub fn mark_webview_page_ready(trigger: On<BinReceive<PageReady>>, mut commands: Commands) {
    commands
        .entity(trigger.event().webview)
        .insert(trigger.event().payload);
}

fn embedded_default_document(host: &str, index_file_path: &str) -> String {
    let h = host.trim().trim_matches('/');
    if h.is_empty() {
        return index_file_path.to_string();
    }
    format!("{h}/{index_file_path}")
}

fn macos_resources_dir_from_exe(exe: &Path) -> Option<PathBuf> {
    let macos_dir = exe.parent()?;
    if macos_dir.file_name()? != "MacOS" {
        return None;
    }
    let contents_dir = macos_dir.parent()?;
    if contents_dir.file_name()? != "Contents" {
        return None;
    }
    Some(contents_dir.join("Resources"))
}

fn current_app_resources_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| macos_resources_dir_from_exe(&exe))
}

fn packaged_page_root(resources_dir: Option<&Path>, host: &str) -> Option<PathBuf> {
    let h = host.trim().trim_matches('/');
    if h.is_empty() {
        return None;
    }
    let root = resources_dir?.join("webview-apps");
    let host_root = root.join(h);
    if host_root.is_dir() {
        return Some(host_root);
    }
    let shared_root = root.join("_shared");
    shared_root.is_dir().then_some(shared_root)
}

fn embed_page_static_assets(
    manifests: Query<&PageManifest>,
    mut reg: ResMut<EmbeddedAssetRegistry>,
) {
    let resources_dir = current_app_resources_dir();
    for manifest in &manifests {
        let bundle_root = manifest.bundle_root(resources_dir.as_deref());
        if !bundle_root.is_dir() {
            bevy::log::warn!("PagePlugin: skip {:?}: not a directory", bundle_root);
            continue;
        }
        let host_trim = manifest.host.trim().trim_matches('/');
        let prefix = if host_trim.is_empty() {
            None
        } else {
            Some(PathBuf::from(host_trim))
        };
        if let Err(e) = embed_dir_recursive(
            &mut reg,
            &bundle_root,
            &bundle_root,
            None,
            prefix.as_deref(),
        ) {
            bevy::log::error!("PagePlugin: failed to embed {:?}: {e}", bundle_root);
        }
    }
}

fn embed_dir_recursive(
    reg: &mut EmbeddedAssetRegistry,
    root_dir: &Path,
    cur: &Path,
    map_root_index_to: Option<&Path>,
    embed_path_prefix: Option<&Path>,
) -> std::io::Result<()> {
    let read_dir = match std::fs::read_dir(cur) {
        Ok(rd) => rd,
        Err(e) if cur == root_dir => return Err(e),
        Err(_) => return Ok(()),
    };
    for e in read_dir.flatten() {
        let p = e.path();
        if p.is_dir() {
            embed_dir_recursive(reg, root_dir, &p, map_root_index_to, embed_path_prefix)?;
        } else {
            let Ok(rel) = p.strip_prefix(root_dir) else {
                continue;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let mut embedded_path: PathBuf = if rel_str == "index.html" {
                map_root_index_to
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from(&rel_str))
            } else {
                PathBuf::from(&rel_str)
            };
            if let Some(prefix) = embed_path_prefix {
                embedded_path = prefix.join(&embedded_path);
            }
            let bytes = std::fs::read(&p)?;
            reg.insert_asset(p, embedded_path.as_path(), bytes);
        }
    }
    Ok(())
}

#[derive(Message, Debug, Clone)]
pub struct SettingsPageSpawnRequest {
    pub target_stack: Entity,
}

#[derive(Message, Debug, Clone)]
pub struct SpacesPageSpawnRequest {
    pub target_stack: Entity,
}

#[cfg(test)]
mod host_history_tests {
    use super::*;

    impl HostHistory {
        fn walked(urls: &[&str]) -> Self {
            let mut history = Self::default();
            for url in urls {
                history.observe(url, 0);
            }
            history
        }

        fn url(&self) -> &str {
            self.entries[self.cursor].url.as_str()
        }
    }

    #[test]
    fn a_repeat_of_the_current_url_does_not_add_an_entry() {
        let mut history = HostHistory::walked(&["a", "b"]);
        history.observe("b", 42);

        assert!(!history.can_go_forward());
        assert_eq!(history.url(), "b");
        history.step(HostHistoryDelta::Back).expect("a is behind b");
        assert_eq!(history.url(), "a");
        assert!(!history.can_go_back());
    }

    #[test]
    fn going_back_restores_the_scroll_position_the_entry_was_left_at() {
        let mut history = HostHistory::default();
        history.observe("a", 0);
        history.observe("a", 120);
        history.observe("b", 0);

        let entry = history.step(HostHistoryDelta::Back).expect("a is behind b");

        assert_eq!(entry.top_line, 120);
    }

    #[test]
    fn a_new_visit_drops_everything_ahead_of_the_cursor() {
        let mut history = HostHistory::walked(&["a", "b", "c"]);
        history.step(HostHistoryDelta::Back);
        history.step(HostHistoryDelta::Back);

        history.observe("d", 0);

        assert!(!history.can_go_forward());
        assert_eq!(history.url(), "d");
        history.step(HostHistoryDelta::Back);
        assert_eq!(history.url(), "a");
    }

    #[test]
    fn the_oldest_entries_fall_off_once_the_stack_is_full() {
        let urls: Vec<String> = (0..HostHistory::CAPACITY + 10)
            .map(|n| n.to_string())
            .collect();
        let mut history = HostHistory::default();
        for url in &urls {
            history.observe(url, 0);
        }

        assert_eq!(history.entries.len(), HostHistory::CAPACITY);
        assert_eq!(history.url(), urls.last().unwrap());
        for _ in 0..HostHistory::CAPACITY {
            history.step(HostHistoryDelta::Back);
        }
        assert_eq!(history.url(), urls[urls.len() - HostHistory::CAPACITY]);
    }

    #[test]
    fn a_step_past_either_end_reports_nothing_and_stays_put() {
        let mut history = HostHistory::walked(&["a"]);

        assert!(history.step(HostHistoryDelta::Back).is_none());
        assert!(history.step(HostHistoryDelta::Forward).is_none());
        assert_eq!(history.url(), "a");
    }

    #[test]
    fn a_step_names_the_webview_whose_history_moved() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(HostHistoryPlugin);
        let owner = app.world_mut().spawn(HostHistory::walked(&["a", "b"])).id();
        let stranger = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(HostHistoryStep {
            webview: stranger,
            delta: HostHistoryDelta::Back,
        });
        app.world_mut().write_message(HostHistoryStep {
            webview: owner,
            delta: HostHistoryDelta::Back,
        });

        app.update();

        let messages = app
            .world()
            .resource::<bevy::ecs::message::Messages<HostHistoryTraversed>>();
        let mut cursor = messages.get_cursor();
        let traversed: Vec<_> = cursor.read(messages).collect();
        assert_eq!(traversed.len(), 1);
        assert_eq!(traversed[0].webview, owner);
        assert_eq!(traversed[0].entry.url, "a");
    }
}

#[cfg(test)]
mod page_ready_tests {
    use super::*;

    #[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    struct PageReadyPayloadProbe {}

    #[test]
    fn page_ready_cross_type_rkyv_compat() {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&PageReadyPayloadProbe {}).expect("ser");
        println!("PageReady archive byte length: {}", bytes.len());
        println!("PageReady archive bytes: {:?}", &bytes[..]);
        let _decoded =
            rkyv::from_bytes::<PageReady, rkyv::rancor::Error>(&bytes).expect("cross-type decode");
    }

    #[test]
    fn page_ready_self_rkyv_roundtrip() {
        let original = PageReady {};
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        println!("PageReady self archive byte length: {}", bytes.len());
        let _decoded =
            rkyv::from_bytes::<PageReady, rkyv::rancor::Error>(&bytes).expect("self decode");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_manifest_url_derives_from_host() {
        let manifest = PageManifest {
            host: "settings",
            title: "Settings",
            title_message_id: Some("settings-title"),
            replaces_command: None,
            keywords: &["preferences"],
            icon: Some(crate::icon::BuiltinIcon::Settings),
            command_bar: true,
        };
        assert_eq!(manifest.url(), "vmux://settings/");
    }

    #[test]
    fn packaged_page_root_uses_resources_webview_host_dir() {
        let root =
            std::env::temp_dir().join(format!("vmux-webview-app-test-{}", std::process::id()));
        let host_dir = root.join("webview-apps").join("terminal");
        std::fs::create_dir_all(&host_dir).unwrap();

        let found = packaged_page_root(Some(&root), "terminal");

        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(found, Some(host_dir));
    }

    #[test]
    fn packaged_page_root_ignores_missing_host_dir() {
        let root = std::env::temp_dir().join(format!(
            "vmux-webview-app-missing-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let found = packaged_page_root(Some(&root), "terminal");

        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(found, None);
    }

    #[test]
    fn page_manifest_registers_host() {
        let mut app = App::new();
        app.world_mut().spawn(PageManifest {
            host: "history",
            title: "History",
            title_message_id: Some("history-title"),
            replaces_command: Some("browser_open_history"),
            keywords: &["recent", "visited"],
            icon: Some(crate::icon::BuiltinIcon::Clock),
            command_bar: true,
        });
        let mut query = app.world_mut().query::<&PageManifest>();

        let hosts = bevy_cef_core::prelude::CefEmbeddedHosts(
            query
                .iter(app.world())
                .map(PageManifest::embedded_host)
                .collect(),
        );

        assert!(hosts.entry_for_host("history").is_some());
    }

    #[test]
    fn registered_hosts_fall_back_to_the_stylesheet_bundle() {
        let manifest = PageManifest {
            host: "history",
            title: "History",
            title_message_id: Some("history-title"),
            replaces_command: Some("browser_open_history"),
            keywords: &["recent", "visited"],
            icon: Some(crate::icon::BuiltinIcon::Clock),
            command_bar: true,
        };

        assert_eq!(
            manifest.bundle_root(None),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vmux_ui/dist")
        );
    }

    #[test]
    fn packaged_page_root_falls_back_to_shared_webview_dist() {
        let root = std::env::temp_dir().join(format!(
            "vmux-webview-app-shared-test-{}",
            std::process::id()
        ));
        let shared = root.join("webview-apps").join("_shared");
        std::fs::create_dir_all(&shared).unwrap();

        let found = packaged_page_root(Some(&root), "history");

        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(found, Some(shared));
    }

    #[test]
    fn macos_resources_dir_resolves_from_bundle_executable() {
        let exe = Path::new("/Applications/Vmux.app/Contents/MacOS/Vmux");

        let resources = macos_resources_dir_from_exe(exe);

        assert_eq!(
            resources,
            Some(PathBuf::from("/Applications/Vmux.app/Contents/Resources"))
        );
    }
}
