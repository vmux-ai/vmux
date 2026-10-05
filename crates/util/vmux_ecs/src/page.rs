use bevy::ecs::system::SystemParam;
use bevy::{
    asset::io::embedded::EmbeddedAssetRegistry,
    prelude::{
        App, Commands, Component, Entity, IntoScheduleConfigs, Message, MessageReader,
        MessageWriter, On, Plugin, PreStartup, Query, ResMut, Startup, SystemSet, Update, With,
    },
};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_cef_core::prelude::CefEmbeddedHost;
pub use vmux_api::PageReady;

use crate::page_driver::PageAssets;

pub struct PagePlugin;

impl Plugin for PagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(PageReady,)>::default())
            .add_plugins(HostHistoryPlugin)
            .add_observer(mark_webview_ready)
            .configure_sets(Startup, PageEmbedSet)
            .add_systems(Startup, embed_static_assets.in_set(PageEmbedSet));
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageManifest {
    pub route: &'static str,
    pub url: &'static str,
    pub asset_host: &'static str,
    pub owns_subtree: bool,
    pub title: &'static str,
    pub title_message_id: Option<&'static str>,
    pub replaces_command: Option<&'static str>,
    pub keywords: &'static [&'static str],
    pub icon: Option<vmux_api::BuiltinIcon>,
    pub command_bar: bool,
    pub startup: bool,
    pub reports_title: bool,
    pub placement: PagePlacement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PagePlacement {
    pub group: &'static str,
    pub auxiliary: bool,
    pub refresh: bool,
    pub reuse: PageReuse,
    pub split: Option<PageSplitPreference>,
}

impl PagePlacement {
    pub const DEFAULT: Self = Self {
        group: "browser",
        auxiliary: false,
        refresh: false,
        reuse: PageReuse::Exact,
        split: None,
    };

    pub fn reuses(&self, request_url: &str, existing: Self, existing_url: &str) -> bool {
        if self.group != existing.group {
            return false;
        }
        match self.reuse {
            PageReuse::Exact => request_url == existing_url,
            PageReuse::Route => {
                let Some(request) = vmux_api::VmuxRoute::parse(request_url) else {
                    return false;
                };
                let Some(existing) = vmux_api::VmuxRoute::parse(existing_url) else {
                    return false;
                };
                request.same_page(&existing)
            }
            PageReuse::Fragmentless => {
                let request = request_url.split('#').next().unwrap_or(request_url);
                let existing = existing_url.split('#').next().unwrap_or(existing_url);
                request == existing
            }
        }
    }
}

impl Default for PagePlacement {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageReuse {
    #[default]
    Exact,
    Route,
    Fragmentless,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSplitPreference {
    pub anchor_group: &'static str,
    pub allowed_groups: &'static [&'static str],
    pub axis: PageSplitAxis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageSplitAxis {
    Row,
    Column,
}

#[derive(SystemParam)]
pub struct PagePlacementCatalog<'w, 's> {
    pages: Query<'w, 's, &'static PageManifest>,
}

impl PagePlacementCatalog<'_, '_> {
    pub fn registered(&self, url: &str) -> bool {
        self.pages.iter().any(|page| page.routes(url))
    }

    pub fn resolve(&self, url: &str) -> PagePlacement {
        self.pages
            .iter()
            .find(|page| page.routes(url))
            .map(|page| page.placement)
            .unwrap_or_default()
    }

    pub fn reuses(&self, request_url: &str, existing_url: &str) -> bool {
        self.resolve(request_url)
            .reuses(request_url, self.resolve(existing_url), existing_url)
    }

    pub fn same_group(&self, left: &str, right: &str) -> bool {
        self.resolve(left).group == self.resolve(right).group
    }

    pub fn reports_title(&self, url: &str) -> bool {
        self.pages
            .iter()
            .find(|page| page.routes(url))
            .is_some_and(|page| page.reports_title)
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostsPage;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupPage;

#[derive(SystemParam)]
pub struct StartupPageUrl<'w, 's> {
    pages: Query<'w, 's, &'static PageManifest, With<StartupPage>>,
}

impl StartupPageUrl<'_, '_> {
    pub fn get(&self) -> Option<&'static str> {
        self.pages.single().ok().map(|page| page.url)
    }

    pub fn resolve(&self, candidate: Option<&str>) -> String {
        candidate
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .or_else(|| self.get())
            .unwrap_or_default()
            .to_string()
    }
}

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
pub struct HostedPage {
    pub url: &'static str,
    pub title: &'static str,
    pub owns_subtree: bool,
}

impl HostedPage {
    pub const fn page(url: &'static str, title: &'static str) -> Self {
        Self {
            url,
            title,
            owns_subtree: false,
        }
    }

    pub const fn subtree(url: &'static str, title: &'static str) -> Self {
        Self {
            url,
            title,
            owns_subtree: true,
        }
    }

    pub fn answers_for(&self, url: &str) -> bool {
        PageRoute {
            url: self.url,
            owns_subtree: self.owns_subtree,
        }
        .matches(url)
    }
}

struct PageRoute<'a> {
    url: &'a str,
    owns_subtree: bool,
}

impl PageRoute<'_> {
    fn matches(&self, url: &str) -> bool {
        if let Some(scheme) = self.url.strip_suffix("://") {
            return url
                .split_once(':')
                .is_some_and(|(candidate, _)| candidate.eq_ignore_ascii_case(scheme));
        }
        if let (Some(base), Some(candidate)) = (
            vmux_api::VmuxRoute::parse(self.url),
            vmux_api::VmuxRoute::parse(url),
        ) {
            return match self.owns_subtree {
                true => candidate.in_subtree(&base),
                false => candidate.same_page(&base),
            };
        }
        let (Ok(base), Ok(candidate)) = (url::Url::parse(self.url), url::Url::parse(url)) else {
            return false;
        };
        if base.scheme() != candidate.scheme() || base.host_str() != candidate.host_str() {
            return false;
        }
        let base_path = base.path().trim_end_matches('/');
        let candidate_path = candidate.path().trim_end_matches('/');
        candidate_path == base_path
            || (self.owns_subtree
                && candidate_path
                    .strip_prefix(base_path)
                    .is_some_and(|suffix| suffix.starts_with('/')))
    }
}

pub struct PageManifestPlugin {
    manifest: PageManifest,
    hosted: Option<HostedPage>,
    route: Option<super::host_spawn::HostSpawnRoute>,
    alias: Option<HostedPage>,
}

impl PageManifestPlugin {
    pub const fn hosted(mut self, hosted: HostedPage) -> Self {
        self.hosted = Some(hosted);
        self
    }

    pub const fn route(mut self, route: super::host_spawn::HostSpawnRoute) -> Self {
        self.route = Some(route);
        self
    }

    pub const fn alias(mut self, hosted: HostedPage) -> Self {
        self.alias = Some(hosted);
        self
    }
}

impl Plugin for PageManifestPlugin {
    fn build(&self, app: &mut App) {
        let manifest = self.manifest;
        let hosted = self.hosted;
        let route = self.route;
        let alias = self.alias;
        app.add_systems(PreStartup, move |mut commands: Commands| {
            if let Some(alias) = alias {
                commands.spawn(alias);
            }
            let mut registration = commands.spawn(manifest);
            if manifest.startup {
                registration.insert(StartupPage);
            }
            if let Some(hosted) = hosted {
                registration.insert(hosted);
            }
            if let Some(route) = route {
                registration.insert(route);
            }
        });
    }

    fn is_unique(&self) -> bool {
        false
    }
}

pub(crate) struct HostHistoryPlugin;

impl Plugin for HostHistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<HostHistoryStep>()
            .add_message::<HostHistoryObserve>()
            .add_message::<HostHistoryTraversed>()
            .configure_sets(
                Update,
                (
                    HostHistorySet::Step,
                    HostHistorySet::Apply,
                    HostHistorySet::Record,
                    HostHistorySet::Commit,
                )
                    .chain(),
            )
            .add_systems(Update, step_host_history.in_set(HostHistorySet::Step))
            .add_systems(Update, observe_host_history.in_set(HostHistorySet::Commit));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HostHistorySet {
    Step,
    Apply,
    Record,
    Commit,
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
}

#[derive(Message, Clone, Debug)]
pub struct HostHistoryObserve {
    pub webview: Entity,
    pub url: String,
    pub top_line: u32,
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

fn observe_host_history(
    mut observations: MessageReader<HostHistoryObserve>,
    mut histories: Query<&mut HostHistory>,
) {
    for observation in observations.read() {
        let Ok(mut history) = histories.get_mut(observation.webview) else {
            continue;
        };
        let cursor = history.cursor;
        if let Some(current) = history.entries.get_mut(cursor)
            && current.url == observation.url
        {
            current.top_line = observation.top_line;
            continue;
        }
        history.entries.truncate(cursor + 1);
        history.entries.push(HostHistoryEntry {
            url: observation.url.clone(),
            top_line: observation.top_line,
        });
        history.cursor = history.entries.len() - 1;
        let overflow = history.entries.len().saturating_sub(HostHistory::CAPACITY);
        if overflow == 0 {
            continue;
        }
        history.entries.drain(..overflow);
        history.cursor -= overflow;
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
        match step.delta {
            HostHistoryDelta::Back => {
                if !history.can_go_back() {
                    continue;
                }
                history.cursor -= 1;
            }
            HostHistoryDelta::Forward => {
                if !history.can_go_forward() {
                    continue;
                }
                history.cursor += 1;
            }
        }
        let Some(entry) = history.entries.get(history.cursor).cloned() else {
            continue;
        };
        traversed.write(HostHistoryTraversed {
            webview: step.webview,
            entry,
        });
    }
}

impl PageManifest {
    pub const fn plugin(self) -> PageManifestPlugin {
        PageManifestPlugin {
            manifest: self,
            hosted: None,
            route: None,
            alias: None,
        }
    }

    pub fn answers_for(&self, url: &str) -> bool {
        vmux_api::UiEventPermissions {
            url: self.url,
            owns_subtree: self.owns_subtree,
            permissions: &[],
        }
        .answers_for(url)
    }

    pub fn routes(&self, url: &str) -> bool {
        PageRoute {
            url: self.route,
            owns_subtree: self.owns_subtree,
        }
        .matches(url)
    }

    pub fn metadata_for(&self, url: impl Into<String>) -> crate::PageMetadata {
        crate::PageMetadata {
            title: self.title.to_string(),
            url: url.into(),
            icon: self
                .icon
                .map(vmux_api::PageIcon::Builtin)
                .unwrap_or_default(),
            bg_color: None,
        }
    }

    pub fn embedded_host(&self) -> CefEmbeddedHost {
        CefEmbeddedHost {
            host: self.asset_host.to_string(),
            default_document: PageAssets::default_document(self.asset_host, "index.html"),
        }
    }

    pub fn url(&self) -> String {
        vmux_api::VmuxRoute::canonical(self.url).unwrap_or_else(|| self.url.to_string())
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageEmbedSet;

fn mark_webview_ready(trigger: On<UiInput<PageReady>>, mut commands: Commands) {
    commands
        .entity(trigger.event().webview)
        .insert(trigger.event().payload);
}

fn embed_static_assets(
    manifests: Query<&PageManifest>,
    registry: Option<ResMut<EmbeddedAssetRegistry>>,
) {
    let Some(mut reg) = registry else {
        return;
    };
    let resources_dir = PageAssets::current_resources();
    for manifest in &manifests {
        let bundle_root = PageAssets::root(manifest, resources_dir.as_deref());
        if !bundle_root.is_dir() {
            bevy::log::warn!("PagePlugin: skip {:?}: not a directory", bundle_root);
            continue;
        }
        if let Err(e) = PageAssets::embed(&mut reg, manifest, &bundle_root) {
            bevy::log::error!("PagePlugin: failed to embed {:?}: {e}", bundle_root);
        }
    }
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
    use bevy::ecs::message::Messages;

    struct HistoryHarness {
        app: App,
        owner: Entity,
    }

    impl HostHistory {
        fn url(&self) -> &str {
            self.entries[self.cursor].url.as_str()
        }
    }

    impl HistoryHarness {
        fn new(urls: &[&str]) -> Self {
            let mut app = App::new();
            app.add_plugins(bevy::MinimalPlugins)
                .add_plugins(HostHistoryPlugin);
            let owner = app.world_mut().spawn(HostHistory::default()).id();
            let mut harness = Self { app, owner };
            for url in urls {
                harness.observe(url, 0);
            }
            harness
        }

        fn history(&self) -> &HostHistory {
            self.app.world().get::<HostHistory>(self.owner).unwrap()
        }

        fn observe(&mut self, url: &str, top_line: u32) {
            self.app.world_mut().write_message(HostHistoryObserve {
                webview: self.owner,
                url: url.to_string(),
                top_line,
            });
            self.app.update();
        }

        fn step(&mut self, delta: HostHistoryDelta) -> Option<HostHistoryEntry> {
            self.app.world_mut().write_message(HostHistoryStep {
                webview: self.owner,
                delta,
            });
            self.app.update();
            self.app
                .world_mut()
                .resource_mut::<Messages<HostHistoryTraversed>>()
                .drain()
                .last()
                .map(|traversed| traversed.entry)
        }
    }

    #[test]
    fn a_repeat_of_the_current_url_does_not_add_an_entry() {
        let mut harness = HistoryHarness::new(&["a", "b"]);
        harness.observe("b", 42);

        assert!(!harness.history().can_go_forward());
        assert_eq!(harness.history().url(), "b");
        harness.step(HostHistoryDelta::Back).expect("a is behind b");
        assert_eq!(harness.history().url(), "a");
        assert!(!harness.history().can_go_back());
    }

    #[test]
    fn going_back_restores_the_scroll_position_the_entry_was_left_at() {
        let mut harness = HistoryHarness::new(&[]);
        harness.observe("a", 0);
        harness.observe("a", 120);
        harness.observe("b", 0);

        let entry = harness.step(HostHistoryDelta::Back).expect("a is behind b");

        assert_eq!(entry.top_line, 120);
    }

    #[test]
    fn a_new_visit_drops_everything_ahead_of_the_cursor() {
        let mut harness = HistoryHarness::new(&["a", "b", "c"]);
        harness.step(HostHistoryDelta::Back);
        harness.step(HostHistoryDelta::Back);

        harness.observe("d", 0);

        assert!(!harness.history().can_go_forward());
        assert_eq!(harness.history().url(), "d");
        harness.step(HostHistoryDelta::Back);
        assert_eq!(harness.history().url(), "a");
    }

    #[test]
    fn the_oldest_entries_fall_off_once_the_stack_is_full() {
        let urls: Vec<String> = (0..HostHistory::CAPACITY + 10)
            .map(|n| n.to_string())
            .collect();
        let mut harness = HistoryHarness::new(&[]);
        for url in &urls {
            harness.observe(url, 0);
        }

        assert_eq!(harness.history().entries.len(), HostHistory::CAPACITY);
        assert_eq!(harness.history().url(), urls.last().unwrap());
        for _ in 0..HostHistory::CAPACITY {
            harness.step(HostHistoryDelta::Back);
        }
        assert_eq!(
            harness.history().url(),
            urls[urls.len() - HostHistory::CAPACITY]
        );
    }

    #[test]
    fn a_step_past_either_end_reports_nothing_and_stays_put() {
        let mut harness = HistoryHarness::new(&["a"]);

        assert!(harness.step(HostHistoryDelta::Back).is_none());
        assert!(harness.step(HostHistoryDelta::Forward).is_none());
        assert_eq!(harness.history().url(), "a");
    }

    #[test]
    fn a_step_names_the_webview_whose_history_moved() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(HostHistoryPlugin);
        let owner = app.world_mut().spawn(HostHistory::default()).id();
        for url in ["a", "b"] {
            app.world_mut().write_message(HostHistoryObserve {
                webview: owner,
                url: url.to_string(),
                top_line: 0,
            });
            app.update();
        }
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

        let messages = app.world().resource::<Messages<HostHistoryTraversed>>();
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
    use std::path::{Path, PathBuf};

    #[test]
    fn page_manifest_canonicalizes_its_url() {
        let manifest = PageManifest {
            route: "vmux://settings/",
            url: "vmux://settings/",
            asset_host: "settings",
            owns_subtree: false,
            title: "Settings",
            title_message_id: Some("settings-title"),
            replaces_command: None,
            keywords: &["preferences"],
            icon: Some(vmux_api::BuiltinIcon::Settings),
            command_bar: true,
            startup: false,
            reports_title: false,
            placement: PagePlacement::DEFAULT,
        };
        assert_eq!(manifest.url(), "vmux://settings/");
    }

    #[test]
    fn page_manifest_answers_for_its_url_subtree() {
        let manifest = PageManifest {
            route: "vmux://simulator/",
            url: "vmux://simulator/",
            asset_host: "simulator",
            owns_subtree: true,
            title: "Simulator",
            title_message_id: None,
            replaces_command: None,
            keywords: &[],
            icon: Some(vmux_api::BuiltinIcon::Smartphone),
            command_bar: true,
            startup: false,
            reports_title: false,
            placement: PagePlacement::DEFAULT,
        };

        assert!(manifest.answers_for("vmux://simulator/"));
        assert!(manifest.answers_for("vmux://simulator/ios/27.0/iPhone%2017%20Pro"));
        assert!(!manifest.answers_for("vmux://simulators/"));
        assert!(!manifest.answers_for("https://simulator/"));
        assert_eq!(
            manifest.metadata_for("vmux://simulator/ios/27.0").icon,
            vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Smartphone)
        );
    }

    #[test]
    fn scheme_page_answers_for_every_path_in_that_scheme() {
        let hosted = HostedPage::subtree("git://", "Git");

        assert!(hosted.answers_for("git://Users/me/repo"));
        assert!(hosted.answers_for("git:///Users/me/repo"));
        assert!(hosted.answers_for("GIT://Users/me/repo"));
        assert!(!hosted.answers_for("https://example.com"));
    }

    #[test]
    fn packaged_page_root_uses_resources_webview_host_dir() {
        let root =
            std::env::temp_dir().join(format!("vmux-webview-app-test-{}", std::process::id()));
        let host_dir = root.join("webview-apps").join("terminal");
        std::fs::create_dir_all(&host_dir).unwrap();

        let found = PageAssets::packaged_root(Some(&root), "terminal");

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

        let found = PageAssets::packaged_root(Some(&root), "terminal");

        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(found, None);
    }

    #[test]
    fn page_manifest_registers_host() {
        let mut app = App::new();
        let manifest = PageManifest {
            route: "vmux://history/",
            url: "vmux://history/",
            asset_host: "history",
            owns_subtree: false,
            title: "History",
            title_message_id: Some("history-title"),
            replaces_command: Some("browser_open_history"),
            keywords: &["recent", "visited"],
            icon: Some(vmux_api::BuiltinIcon::Clock),
            command_bar: true,
            startup: false,
            reports_title: false,
            placement: PagePlacement::DEFAULT,
        };
        app.add_plugins(
            manifest
                .plugin()
                .hosted(HostedPage::page(manifest.url, manifest.title)),
        );
        app.update();
        let mut query = app.world_mut().query::<&PageManifest>();

        let hosts = bevy_cef_core::prelude::CefEmbeddedHosts(
            query
                .iter(app.world())
                .map(PageManifest::embedded_host)
                .collect(),
        );

        assert!(hosts.entry_for_host("history").is_some());
        let mut registration = app.world_mut().query::<(&PageManifest, &HostedPage)>();
        let (registered, hosted) = registration.single(app.world()).unwrap();
        assert_eq!(*registered, manifest);
        assert_eq!(*hosted, HostedPage::page(manifest.url, manifest.title));
    }

    #[test]
    fn registered_hosts_fall_back_to_the_stylesheet_bundle() {
        let manifest = PageManifest {
            route: "vmux://history/",
            url: "vmux://history/",
            asset_host: "history",
            owns_subtree: false,
            title: "History",
            title_message_id: Some("history-title"),
            replaces_command: Some("browser_open_history"),
            keywords: &["recent", "visited"],
            icon: Some(vmux_api::BuiltinIcon::Clock),
            command_bar: true,
            startup: false,
            reports_title: false,
            placement: PagePlacement::DEFAULT,
        };

        assert_eq!(
            PageAssets::root(&manifest, None),
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

        let found = PageAssets::packaged_root(Some(&root), "history");

        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(found, Some(shared));
    }

    #[test]
    fn macos_resources_dir_resolves_from_bundle_executable() {
        let exe = Path::new("/Applications/Vmux.app/Contents/MacOS/Vmux");

        let resources = PageAssets::resources_from_exe(exe);

        assert_eq!(
            resources,
            Some(PathBuf::from("/Applications/Vmux.app/Contents/Resources"))
        );
    }
}
