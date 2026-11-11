use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::command_bar::{CommandBarResultItem, OpenRequest};
use vmux_api::mcp::{McpServerEntry, McpServerRequest, McpServerStatus, McpServersUiState};
use vmux_ecs::UiStateWrite;
use vmux_ecs::manifest::FeatureManifest;
use vmux_ecs::{CommandBarContribution, CommandBarContributionActivated, CommandBarQueryChanged};
use vmux_ui::i18n::translate;

use crate::connection::McpSnapshotRequest;

pub(crate) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_observer(query)
            .add_observer(receive)
            .add_observer(activate)
            .add_observer(routes)
            .add_observer(open_route);
    }
}

#[derive(Component)]
struct RouteContribution;

fn routes(
    trigger: On<CommandBarQueryChanged>,
    manifests: Query<&FeatureManifest>,
    pages: Query<&vmux_ecs::page::PageManifest>,
    existing: Query<(Entity, &ChildOf), With<RouteContribution>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    for (entity, parent) in &existing {
        if parent.parent() == event.target {
            commands.entity(entity).despawn();
        }
    }
    if event.picker.is_some() {
        return;
    }
    let Some(filter) = RouteQuery::parse(&event.query) else {
        return;
    };
    let mut categories = std::collections::BTreeMap::new();
    for manifest in &manifests {
        for provider in &manifest.tool_providers {
            if provider.route.is_empty() || categories.contains_key(&provider.route) {
                continue;
            }
            let label = if provider.route_title_message_id.is_empty() {
                if provider.route_title.is_empty() {
                    provider.title.clone()
                } else {
                    provider.route_title.clone()
                }
            } else {
                translate(&provider.route_title_message_id)
            };
            categories.insert(provider.route.clone(), (label, provider.rank));
        }
    }
    categories
        .entry("extensions".to_string())
        .or_insert((translate("extensions-title"), 100));
    for (route, (label, rank)) in categories {
        if !filter.matches(&route, &label) {
            continue;
        }
        let url = format!("{}{route}", crate::ToolPlugin::URL);
        if filter.0.is_empty()
            && pages
                .iter()
                .any(|page| page.command_bar && page.url() == url)
        {
            continue;
        }
        commands.spawn((
            Name::new(format!("Tools category: {route}")),
            RouteContribution,
            ChildOf(event.target),
            CommandBarContribution {
                row: CommandBarResultItem {
                    key: format!("tools-route:{route}"),
                    title: format!("{} / {label}", translate("tools-title")),
                    subtitle: url.clone(),
                    url: url.clone(),
                    icon: vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Toolbox),
                    ..Default::default()
                },
                value: url,
                rank,
                close: true,
                pre_filtered: true,
                ..Default::default()
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use vmux_api::open_target::OpenTarget;

    #[derive(Component)]
    struct Opened(OpenRequest);

    fn capture(trigger: On<UiInput<OpenRequest>>, mut commands: Commands) {
        commands
            .entity(trigger.event().webview)
            .insert(Opened(trigger.event().payload.clone()));
    }

    struct Palette {
        app: App,
        view: Entity,
    }

    impl Palette {
        fn create() -> Self {
            let mut app = App::new();
            app.add_plugins(Plugin).add_observer(capture);
            app.world_mut()
                .spawn(FeatureManifest::of::<crate::Feature>());
            app.world_mut().spawn(FeatureManifest::parse(r#"(
                tool_providers: [
                    (id: "agent", title: "Agents", route_title: "Agents", route: "acp", rank: 0),
                    (id: "language", title: "Language servers", route_title: "Language servers · Éditeurs", route: "lsp", rank: 10),
                ],
            )"#));
            let view = app.world_mut().spawn_empty().id();
            Self { app, view }
        }

        fn query(&mut self, query: &str) {
            self.app.world_mut().trigger(CommandBarQueryChanged {
                target: self.view,
                open_id: vmux_api::command_bar::OpenId(1),
                query: query.into(),
                start: false,
                open: None,
                picker: None,
                numbered: false,
            });
            self.app.world_mut().flush();
        }

        fn urls(&mut self) -> BTreeSet<String> {
            let mut rows = self
                .app
                .world_mut()
                .query_filtered::<(&CommandBarContribution, &ChildOf), With<RouteContribution>>();
            rows.iter(self.app.world())
                .filter(|(_, parent)| parent.parent() == self.view)
                .map(|(row, _)| row.value.clone())
                .collect()
        }

        fn activate(&mut self, url: &str, open: OpenTarget) {
            let mut rows = self.app.world_mut().query_filtered::<(Entity, &CommandBarContribution, &ChildOf), With<RouteContribution>>();
            let target = rows
                .iter(self.app.world())
                .find(|(_, row, parent)| parent.parent() == self.view && row.value == url)
                .unwrap()
                .0;
            self.app
                .world_mut()
                .trigger(CommandBarContributionActivated {
                    target,
                    webview: self.view,
                    query: String::new(),
                    open: Some(open),
                });
            self.app.world_mut().flush();
        }
    }

    #[test]
    fn tools_prefix_offers_each_registered_category_once_and_replaces_stale_rows() {
        let mut palette = Palette::create();
        palette.query("vmux://tools/");
        assert_eq!(
            palette.urls(),
            [
                "acp",
                "lsp",
                "homebrew",
                "npm",
                "mcp",
                "dotfiles",
                "extensions"
            ]
            .into_iter()
            .map(|route| format!("vmux://tools/{route}"))
            .collect()
        );
        palette.query("vmux://tools/ls");
        assert_eq!(palette.urls(), BTreeSet::from(["vmux://tools/lsp".into()]));
        palette.query("ÉDITEURS");
        assert_eq!(palette.urls(), BTreeSet::from(["vmux://tools/lsp".into()]));
        palette.query("language");
        assert_eq!(palette.urls(), BTreeSet::from(["vmux://tools/lsp".into()]));
        palette.query("https://example.com");
        assert!(palette.urls().is_empty());
    }

    #[test]
    fn chosen_category_preserves_the_requested_open_target() {
        let mut palette = Palette::create();
        palette.query("tools npm");
        for open in [
            OpenTarget::InPlace,
            OpenTarget::InNewStack,
            OpenTarget::InNewTab,
        ] {
            palette.activate("vmux://tools/npm", open);
            let opened = palette.app.world().get::<Opened>(palette.view).unwrap();
            assert_eq!(opened.0.value, "vmux://tools/npm");
            assert_eq!(opened.0.open, Some(open));
        }
    }

    #[test]
    fn registered_default_page_is_not_duplicated_and_other_contributions_are_preserved() {
        let mut palette = Palette::create();
        palette.app.world_mut().spawn(crate::ToolPlugin::MANIFEST);
        let other = palette
            .app
            .world_mut()
            .spawn((CommandBarContribution::default(), ChildOf(palette.view)))
            .id();
        palette.query("vmux://tools/");
        assert!(!palette.urls().contains("vmux://tools/acp"));
        assert!(palette.app.world().get_entity(other).is_ok());
        palette.query("acp");
        assert_eq!(palette.urls(), BTreeSet::from(["vmux://tools/acp".into()]));
    }
}

fn open_route(
    trigger: On<CommandBarContributionActivated>,
    routes: Query<&CommandBarContribution, With<RouteContribution>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok(route) = routes.get(event.target) else {
        return;
    };
    commands.trigger(UiInput {
        webview: event.webview,
        payload: OpenRequest {
            value: route.value.clone(),
            open: event.open,
        },
    });
}

struct RouteQuery(String);

impl RouteQuery {
    fn parse(query: &str) -> Option<Self> {
        let query = query.trim();
        let root = crate::ToolPlugin::URL.trim_end_matches('/');
        let lower = query.to_lowercase();
        if let Some(rest) = lower.strip_prefix(root) {
            if !rest.is_empty() && !rest.starts_with('/') {
                return None;
            }
            return Some(Self(rest.trim_matches('/').to_string()));
        }
        if lower.contains("://") || lower.starts_with('/') || lower.is_empty() {
            return None;
        }
        let tools = translate("tools-title").to_lowercase();
        if lower == "tools" || lower == tools {
            return Some(Self(String::new()));
        }
        if let Some(rest) = lower.strip_prefix("tools ") {
            return Some(Self(rest.trim().to_string()));
        }
        Some(Self(lower))
    }

    fn matches(&self, route: &str, label: &str) -> bool {
        self.0.is_empty()
            || route.to_ascii_lowercase().contains(&self.0)
            || label.to_lowercase().contains(&self.0)
    }
}

#[derive(Component)]
struct McpContribution(String);

#[derive(Component)]
struct McpCommandBar(String);

fn query(trigger: On<CommandBarQueryChanged>, mut results: McpResults) {
    let event = trigger.event();
    results.clear(event.target);
    let Some(filter) = McpQuery::parse(&event.query) else {
        results
            .commands
            .entity(event.target)
            .remove::<McpCommandBar>();
        return;
    };
    results
        .commands
        .entity(event.target)
        .insert(McpCommandBar(filter.to_string()));
    results.commands.trigger(McpSnapshotRequest {
        target: event.target,
    });
}

fn receive(
    trigger: On<UiStateWrite<McpServersUiState>>,
    active: Query<&McpCommandBar>,
    mut results: McpResults,
) {
    let target = trigger.event().webview();
    let Ok(active) = active.get(target) else {
        return;
    };
    results.clear(target);
    let state = trigger.event().update();
    for (rank, server) in state
        .servers
        .iter()
        .filter(|server| McpServerRow(server).matches(&active.0))
        .enumerate()
    {
        results.commands.spawn((
            Name::new(format!("MCP command-bar row: {}", server.id)),
            CommandBarContribution {
                row: McpServerRow(server).project(state),
                rank: rank as i32,
                close: false,
                pre_filtered: true,
                ..Default::default()
            },
            McpContribution(server.id.clone()),
            ChildOf(target),
        ));
    }
}

fn activate(
    trigger: On<CommandBarContributionActivated>,
    contributions: Query<&McpContribution>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok(contribution) = contributions.get(event.target) else {
        return;
    };
    commands.trigger(UiInput {
        webview: event.webview,
        payload: McpServerRequest {
            id: contribution.0.clone(),
        },
    });
}

#[derive(SystemParam)]
struct McpResults<'w, 's> {
    existing: Query<'w, 's, (Entity, &'static ChildOf), With<McpContribution>>,
    commands: Commands<'w, 's>,
}

impl McpResults<'_, '_> {
    fn clear(&mut self, target: Entity) {
        for (entity, parent) in &self.existing {
            if parent.parent() == target {
                self.commands.entity(entity).despawn();
            }
        }
    }
}

struct McpQuery;

impl McpQuery {
    fn parse(query: &str) -> Option<&str> {
        let rest = query.trim().strip_prefix("/mcp")?;
        if rest.is_empty() {
            return Some("");
        }
        rest.chars()
            .next()?
            .is_whitespace()
            .then(|| rest.trim_start())
    }
}

struct McpServerRow<'a>(&'a McpServerEntry);

impl McpServerRow<'_> {
    fn matches(&self, filter: &str) -> bool {
        let filter = filter.to_ascii_lowercase();
        filter.is_empty()
            || self.0.id.to_ascii_lowercase().contains(&filter)
            || self.0.name.to_ascii_lowercase().contains(&filter)
            || self.0.description.to_ascii_lowercase().contains(&filter)
    }

    fn project(&self, state: &McpServersUiState) -> CommandBarResultItem {
        let server = self.0;
        CommandBarResultItem {
            key: server.id.clone(),
            title: server.name.clone(),
            subtitle: server.description.clone(),
            trailing: "\u{21b5}".to_string(),
            active: server.status == McpServerStatus::Connected,
            disabled: state.loading || state.pending.is_some(),
            ..Default::default()
        }
    }
}
