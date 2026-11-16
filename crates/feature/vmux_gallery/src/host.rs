use bevy::prelude::{
    Add, App, Changed, Commands, Component, Entity, EntityEvent, IntoScheduleConfigs, On, Or,
    Plugin, Query, Update,
};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::manifest::{FeatureManifestSource, FeaturePlugin};
use vmux_ecs::{PageMetadata, UiState, UiStatePlugin, UiStateWrite};
use vmux_layout::hosted_page::HostedUiPlugin;

use crate::{
    Examples, GalleryAgentRequest, GalleryAttachRequest, GalleryBranchRequest,
    GalleryCheckedRequest, GalleryChoiceRequest, GalleryClickRequest, GalleryCursorRequest,
    GalleryDialogRequest, GalleryDisabledRequest, GalleryEffortRequest, GalleryMenuRequest,
    GalleryModelRequest, GalleryPasteRequest, GalleryPermissionRequest, GalleryProgressRequest,
    GalleryProjectRequest, GalleryRemoveAttachmentRequest, GalleryResetRequest,
    GalleryStatusRequest, GalleryStopRequest, GallerySubmitRequest, GalleryTextRequest,
    GalleryUiState, Story,
};
use vmux_api::address::{AddressCrumb, AddressIntent, AddressOption, PageAddress};
use vmux_api::command_bar::CommandPaletteAgent;
use vmux_api::mcp::{McpServerEntry, McpServerStatus};
use vmux_api::prompt_media::{PromptComposerAttachment, PromptMediaOption};
use vmux_api::protocol::AcpModeOption;
use vmux_api::session::ModelOptionEntry;
use vmux_api::space::{ProjectBranch, ProjectRow};

#[vmux_page::page]
pub struct GalleryPlugin;

impl Plugin for GalleryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FeaturePlugin::<Self>::default(),
            HostedUiPlugin::<Preview>::new(Self::MANIFEST),
            UiEventPlugin::<(
                GalleryDisabledRequest,
                GalleryTextRequest,
                GalleryCheckedRequest,
                GalleryChoiceRequest,
                GalleryDialogRequest,
                GalleryProgressRequest,
                GalleryClickRequest,
                GalleryResetRequest,
            )>::default(),
            UiEventPlugin::<(
                GalleryMenuRequest,
                GalleryCursorRequest,
                GalleryAgentRequest,
                GalleryModelRequest,
                GalleryEffortRequest,
                GalleryPermissionRequest,
                GalleryProjectRequest,
                GalleryBranchRequest,
            )>::default(),
            UiEventPlugin::<(
                GallerySubmitRequest,
                GalleryStopRequest,
                GalleryAttachRequest,
                GalleryRemoveAttachmentRequest,
                GalleryPasteRequest,
                GalleryStatusRequest,
            )>::default(),
            PreviewPlugin,
        ));
        #[cfg(ui)]
        app.add_plugins(crate::ui::GalleryPage::plugin());
    }
}

impl FeatureManifestSource for GalleryPlugin {
    const SOURCE: &'static str = include_str!("feature.ron");
}

struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiStatePlugin::<GalleryUiState>::default())
            .add_observer(initialize)
            .add_observer(disable)
            .add_observer(edit)
            .add_observer(check)
            .add_observer(choose)
            .add_observer(open)
            .add_observer(progress)
            .add_observer(click)
            .add_observer(reset)
            .add_observer(menu)
            .add_observer(cursor)
            .add_observer(agent)
            .add_observer(model)
            .add_observer(effort)
            .add_observer(permission)
            .add_observer(project)
            .add_observer(branch)
            .add_observer(submit)
            .add_observer(stop)
            .add_observer(attach)
            .add_observer(remove_attachment)
            .add_observer(paste)
            .add_observer(status)
            .add_systems(
                Update,
                (sync, publish)
                    .chain()
                    .after(bevy_cef::prelude::UiEventSystems::Flush),
            );
    }
}

#[derive(Component)]
#[require(UiState<GalleryUiState>)]
struct Preview {
    story: Story,
    disabled: bool,
    text: String,
    checked: bool,
    choice: String,
    dialog_open: bool,
    progress: f64,
    interaction_count: u64,
    last_event: String,
    revision: u64,
    menu: String,
    cursor: usize,
    agent: String,
    model: String,
    effort: String,
    permission: String,
    project: String,
    branch: String,
    running: bool,
    has_attachment: bool,
    status: String,
    submitted: String,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            story: Story::Button,
            disabled: false,
            text: String::new(),
            checked: false,
            choice: "one".to_string(),
            dialog_open: false,
            progress: 40.0,
            interaction_count: 0,
            last_event: String::new(),
            revision: 0,
            menu: String::new(),
            cursor: 0,
            agent: "codex".into(),
            model: "balanced".into(),
            effort: "medium".into(),
            permission: "default".into(),
            project: "/example/vmux".into(),
            branch: "main".into(),
            running: false,
            has_attachment: false,
            status: "idle".into(),
            submitted: String::new(),
        }
    }
}

impl Preview {
    fn reset(&mut self, story: Story) {
        let revision = self.revision.wrapping_add(1);
        *self = Self::default();
        self.story = story;
        self.revision = revision;
    }

    fn record(&mut self, event: &str) {
        self.interaction_count = self.interaction_count.saturating_add(1);
        self.last_event = event.to_string();
    }

    fn accepts(&self, revision: u64) -> bool {
        self.revision == revision && !self.disabled
    }

    fn project(&self, url: &str) -> GalleryUiState {
        GalleryUiState {
            address: self.address(url),
            examples: self.examples(),
            story: self.story,
            disabled: self.disabled,
            text: self.text.clone(),
            checked: self.checked,
            choice: self.choice.clone(),
            dialog_open: self.dialog_open,
            progress: self.progress,
            interaction_count: self.interaction_count,
            last_event: self.last_event.clone(),
            revision: self.revision,
            menu: self.menu.clone(),
            cursor: self.cursor,
            agent: self.agent.clone(),
            model: self.model.clone(),
            effort: self.effort.clone(),
            permission: self.permission.clone(),
            project: self.project.clone(),
            branch: self.branch.clone(),
            running: self.running,
            has_attachment: self.has_attachment,
            status: self.status.clone(),
            submitted: self.submitted.clone(),
        }
    }
}

impl Preview {
    fn address(&self, url: &str) -> PageAddress {
        let mut options = Vec::new();
        for story in Story::ALL {
            options.push(AddressOption {
                label: story.name().into(),
                intent: Some(AddressIntent::Navigate { url: story.url() }),
                ..Default::default()
            });
        }
        PageAddress {
            url: url.to_string(),
            current: AddressOption {
                label_key: "gallery-title".into(),
                icon: Some(vmux_api::BuiltinIcon::Layers),
                intent: Some(AddressIntent::Navigate {
                    url: GalleryPlugin::URL.into(),
                }),
                ..Default::default()
            },
            fixed: vec![AddressCrumb {
                entry: AddressOption {
                    label: self.story.name().into(),
                    intent: Some(AddressIntent::Navigate {
                        url: self.story.url(),
                    }),
                    ..Default::default()
                },
                options,
                active: true,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn composer_examples(&self) -> Examples {
        let mut examples = Examples::default();
        for (id, title) in [("codex", "Codex"), ("claude", "Claude")] {
            examples.agents.push(CommandPaletteAgent {
                url: id.into(),
                title: title.into(),
                ..Default::default()
            });
        }
        for (id, name) in [("balanced", "Balanced"), ("fast", "Fast")] {
            examples.models.push(ModelOptionEntry {
                id: id.into(),
                name: name.into(),
                description: format!("Example model: {id}"),
            });
        }
        for (id, name) in [("default", "Default"), ("plan", "Plan")] {
            examples.modes.push(AcpModeOption {
                id: id.into(),
                name: name.into(),
                description: Some(format!("Example permission mode: {id}")),
            });
        }
        for (path, label) in [("/example/vmux", "vmux"), ("/example/library", "library")] {
            examples.projects.push(ProjectRow {
                path: path.into(),
                label: label.into(),
                display_path: path.into(),
                branch: "main".into(),
                is_active: path == self.project,
                ..Default::default()
            });
        }
        for branch in ["main", "feature/ui"] {
            examples.branches.push(ProjectBranch {
                branch: branch.into(),
                checkout: "/example/vmux".into(),
                label: branch.into(),
                insertions: 12,
                deletions: 3,
            });
        }
        examples
    }

    fn examples(&self) -> Examples {
        let mut examples = self.composer_examples();
        for (name, is_dir) in [("src", true), ("example.rs", false), ("Cargo.toml", false)] {
            examples.files.push(vmux_ecs::event::FileDirEntry {
                name: name.into(),
                path: format!("/example/{name}"),
                is_dir,
            });
        }
        for (name, is_dir) in [("example.rs", false), ("assets", true)] {
            examples.media.push(PromptMediaOption {
                key: name.into(),
                name: name.into(),
                display_path: format!("/example/{name}"),
                label: "RS".into(),
                is_dir,
                ..Default::default()
            });
        }
        if self.has_attachment {
            examples.attachments.push(PromptComposerAttachment {
                key: "example.rs".into(),
                name: "example.rs".into(),
                label: "RS".into(),
                remove_index: Some(0),
                ..Default::default()
            });
        }
        examples.mcp.loaded = true;
        for (id, status) in [
            ("filesystem", McpServerStatus::Available),
            ("search", McpServerStatus::Connected),
            ("calendar", McpServerStatus::AuthenticationRequired),
        ] {
            examples.mcp.servers.push(McpServerEntry {
                id: id.into(),
                name: id.into(),
                description: format!("Example MCP server: {id}"),
                status,
            });
        }
        examples
    }
}

fn initialize(event: On<Add, Preview>, mut previews: Query<(&PageMetadata, &mut Preview)>) {
    let Ok((metadata, mut preview)) = previews.get_mut(event.event_target()) else {
        return;
    };
    let story = Story::from_url(&metadata.url).unwrap_or(Story::Button);
    if preview.story != story {
        preview.reset(story);
    }
}

fn sync(mut previews: Query<(&PageMetadata, &mut Preview), Changed<PageMetadata>>) {
    for (metadata, mut preview) in &mut previews {
        let story = Story::from_url(&metadata.url).unwrap_or(Story::Button);
        if preview.story != story {
            preview.reset(story);
        }
    }
}

fn disable(event: On<UiInput<GalleryDisabledRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    preview.disabled = event.payload.disabled;
    if preview.disabled {
        preview.menu.clear();
    }
}

fn edit(event: On<UiInput<GalleryTextRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision) {
        return;
    }
    preview.text = event.payload.text.chars().take(4096).collect();
    preview.record("GalleryTextRequest");
}

fn check(event: On<UiInput<GalleryCheckedRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision) {
        return;
    }
    preview.checked = event.payload.checked;
    preview.record("GalleryCheckedRequest");
}

fn choose(event: On<UiInput<GalleryChoiceRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision) {
        return;
    }
    if !matches!(
        event.payload.choice.as_str(),
        "one" | "two" | "three" | "list" | "grid"
    ) {
        return;
    }
    preview.choice = event.payload.choice.clone();
    preview.record("GalleryChoiceRequest");
}

fn open(event: On<UiInput<GalleryDialogRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if preview.revision != event.payload.revision || (preview.disabled && event.payload.open) {
        return;
    }
    preview.dialog_open = event.payload.open;
    preview.record("GalleryDialogRequest");
}

fn progress(event: On<UiInput<GalleryProgressRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if preview.revision != event.payload.revision
        || preview.disabled
        || !event.payload.value.is_finite()
    {
        return;
    }
    preview.progress = event.payload.value.clamp(0.0, 100.0);
    preview.record("GalleryProgressRequest");
}

fn click(event: On<UiInput<GalleryClickRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision) {
        return;
    }
    preview.record(&format!(
        "GalleryClickRequest: {}",
        event.payload.label.chars().take(64).collect::<String>()
    ));
}

fn reset(event: On<UiInput<GalleryResetRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    let story = preview.story;
    preview.reset(story);
}

fn menu(event: On<UiInput<GalleryMenuRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if preview.revision != event.payload.revision
        || (preview.disabled && !event.payload.menu.is_empty())
    {
        return;
    }
    if !matches!(
        event.payload.menu.as_str(),
        "" | "agent"
            | "model"
            | "effort"
            | "permission"
            | "project"
            | "branch"
            | "dropdown"
            | "dropdown-selector"
            | "breadcrumb"
            | "prompt"
            | "prompt-flat"
            | "mcp"
            | "project-select"
            | "manager-search"
    ) {
        return;
    }
    preview.menu = event.payload.menu.clone();
    preview.cursor = 0;
}

fn cursor(event: On<UiInput<GalleryCursorRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision) || event.payload.cursor > 16 {
        return;
    }
    preview.cursor = event.payload.cursor;
}

fn agent(event: On<UiInput<GalleryAgentRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision)
        || !matches!(event.payload.value.as_str(), "codex" | "claude")
    {
        return;
    }
    preview.agent = event.payload.value.clone();
    preview.menu.clear();
    preview.record("GalleryAgentRequest");
}

fn model(event: On<UiInput<GalleryModelRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision)
        || !matches!(event.payload.value.as_str(), "balanced" | "fast")
    {
        return;
    }
    preview.model = event.payload.value.clone();
    preview.menu.clear();
    preview.record("GalleryModelRequest");
}

fn effort(event: On<UiInput<GalleryEffortRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision)
        || !matches!(event.payload.value.as_str(), "" | "low" | "medium" | "high")
    {
        return;
    }
    preview.effort = event.payload.value.clone();
    preview.menu.clear();
    preview.record("GalleryEffortRequest");
}

fn permission(event: On<UiInput<GalleryPermissionRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision)
        || !matches!(event.payload.value.as_str(), "default" | "plan")
    {
        return;
    }
    preview.permission = event.payload.value.clone();
    preview.menu.clear();
    preview.record("GalleryPermissionRequest");
}

fn project(event: On<UiInput<GalleryProjectRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision)
        || !matches!(
            event.payload.value.as_str(),
            "/example/vmux" | "/example/library"
        )
    {
        return;
    }
    preview.project = event.payload.value.clone();
    preview.menu.clear();
    preview.record("GalleryProjectRequest");
}

fn branch(event: On<UiInput<GalleryBranchRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision)
        || !matches!(event.payload.value.as_str(), "main" | "feature/ui")
    {
        return;
    }
    preview.branch = event.payload.value.clone();
    preview.menu.clear();
    preview.record("GalleryBranchRequest");
}

fn submit(event: On<UiInput<GallerySubmitRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision) || preview.running || preview.text.trim().is_empty()
    {
        return;
    }
    preview.submitted = preview.text.clone();
    preview.text.clear();
    preview.running = true;
    preview.status = "streaming".into();
    preview.record("GallerySubmitRequest");
}

fn stop(event: On<UiInput<GalleryStopRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision) || !preview.running {
        return;
    }
    preview.running = false;
    preview.status = "idle".into();
    preview.record("GalleryStopRequest");
}

fn attach(event: On<UiInput<GalleryAttachRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision) {
        return;
    }
    preview.has_attachment = true;
    preview.record("GalleryAttachRequest");
}

fn remove_attachment(
    event: On<UiInput<GalleryRemoveAttachmentRequest>>,
    mut previews: Query<&mut Preview>,
) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision)
        || event.payload.index != 0
        || !preview.has_attachment
    {
        return;
    }
    preview.has_attachment = false;
    preview.record("GalleryRemoveAttachmentRequest");
}

fn paste(event: On<UiInput<GalleryPasteRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision) {
        return;
    }
    preview.text = "Example clipboard content".into();
    preview.record("GalleryPasteRequest");
}

fn status(event: On<UiInput<GalleryStatusRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if !preview.accepts(event.payload.revision)
        || !matches!(
            event.payload.status.as_str(),
            "idle" | "streaming" | "awaiting" | "installing" | "errored" | "in_use"
        )
    {
        return;
    }
    preview.status = event.payload.status.clone();
}

type ChangedViews<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Preview, Option<&'static PageMetadata>),
    Or<(Changed<Preview>, Changed<PageMetadata>)>,
>;

fn publish(previews: ChangedViews, mut commands: Commands) {
    for (entity, preview, metadata) in &previews {
        let url = metadata.map_or_else(|| preview.story.url(), |metadata| metadata.url.clone());
        commands.trigger(UiStateWrite::<GalleryUiState>::from_event(
            entity,
            &preview.project(&url),
        ));
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_routes_drive_selection_and_discard_stale_preview_callbacks() {
        let mut app = App::new();
        app.add_plugins(PreviewPlugin);
        let page = app
            .world_mut()
            .spawn(PageMetadata {
                url: Story::Input.url(),
                title: "Input".into(),
                ..Default::default()
            })
            .id();
        app.world_mut().entity_mut(page).insert(Preview::default());
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryTextRequest {
                revision: 1,
                text: "hello".into(),
            },
        });
        app.update();
        assert_eq!(app.world().get::<Preview>(page).unwrap().text, "hello");
        app.world_mut().get_mut::<PageMetadata>(page).unwrap().url = Story::PromptComposer.url();
        app.update();
        let preview = app.world().get::<Preview>(page).unwrap();
        assert_eq!(preview.story, Story::PromptComposer);
        assert!(preview.text.is_empty());
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryTextRequest {
                revision: 1,
                text: "stale".into(),
            },
        });
        app.update();
        assert!(app.world().get::<Preview>(page).unwrap().text.is_empty());
        let address = app
            .world()
            .get::<Preview>(page)
            .unwrap()
            .address("vmux://ui/composer");
        assert_eq!(address.url, "vmux://ui/composer");
        assert_eq!(address.fixed[0].entry.label, "PromptComposer");
        app.world_mut().get_mut::<PageMetadata>(page).unwrap().url = Story::Input.url();
        app.update();
        assert_eq!(
            app.world().get::<Preview>(page).unwrap().story,
            Story::Input
        );
    }

    #[test]
    fn composer_submit_attachment_and_stop_remain_isolated_preview_operations() {
        let mut app = App::new();
        app.add_plugins(PreviewPlugin);
        let first = app.world_mut().spawn(Preview::default()).id();
        let second = app.world_mut().spawn(Preview::default()).id();
        app.world_mut().trigger(UiInput {
            webview: first,
            payload: GalleryTextRequest {
                revision: 0,
                text: "Explain this component".into(),
            },
        });
        app.world_mut().trigger(UiInput {
            webview: first,
            payload: GalleryAttachRequest { revision: 0 },
        });
        app.world_mut().trigger(UiInput {
            webview: first,
            payload: GallerySubmitRequest { revision: 0 },
        });
        app.update();
        let preview = app.world().get::<Preview>(first).unwrap();
        assert!(preview.running);
        assert_eq!(preview.submitted, "Explain this component");
        assert!(preview.text.is_empty());
        assert_eq!(preview.examples().attachments.len(), 1);
        assert!(!app.world().get::<Preview>(second).unwrap().running);
        app.world_mut().trigger(UiInput {
            webview: first,
            payload: GalleryStopRequest { revision: 0 },
        });
        app.world_mut().trigger(UiInput {
            webview: first,
            payload: GalleryRemoveAttachmentRequest {
                revision: 0,
                index: 0,
            },
        });
        app.update();
        let preview = app.world().get::<Preview>(first).unwrap();
        assert!(!preview.running);
        assert!(preview.examples().attachments.is_empty());
    }

    #[test]
    fn disabled_controls_refuse_preview_input_and_reset_restores_controls() {
        let mut app = App::new();
        app.add_plugins(PreviewPlugin);
        let page = app.world_mut().spawn(Preview::default()).id();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryDisabledRequest { disabled: true },
        });
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryAttachRequest { revision: 0 },
        });
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryTextRequest {
                revision: 0,
                text: "ignored".into(),
            },
        });
        app.update();
        assert!(!app.world().get::<Preview>(page).unwrap().has_attachment);
        assert_eq!(
            app.world().get::<Preview>(page).unwrap().interaction_count,
            0
        );
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryResetRequest,
        });
        app.update();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryTextRequest {
                revision: 1,
                text: "accepted".into(),
            },
        });
        app.update();
        assert_eq!(app.world().get::<Preview>(page).unwrap().text, "accepted");
    }

    #[test]
    fn progress_refuses_non_finite_values_and_clamps_finite_input() {
        let mut app = App::new();
        app.add_plugins(PreviewPlugin);
        let page = app.world_mut().spawn(Preview::default()).id();
        for (value, expected) in [
            (150.0, 100.0),
            (f64::NAN, 100.0),
            (f64::INFINITY, 100.0),
            (-10.0, 0.0),
        ] {
            app.world_mut().trigger(UiInput {
                webview: page,
                payload: GalleryProgressRequest { revision: 0, value },
            });
            app.update();
            assert_eq!(app.world().get::<Preview>(page).unwrap().progress, expected);
        }
        assert_eq!(
            app.world().get::<Preview>(page).unwrap().interaction_count,
            2
        );
    }

    #[test]
    fn component_urls_parse_root_queries_and_reject_foreign_or_unknown_paths() {
        for (url, expected) in [
            ("vmux://ui", Some(Story::Button)),
            ("vmux://ui/composer?state=send", Some(Story::PromptComposer)),
            ("vmux://ui/agent-menu/#selection", Some(Story::AgentMenu)),
            ("vmux://ui.other/composer", None),
            ("https://ui/composer", None),
            ("vmux://ui/unknown", None),
        ] {
            assert_eq!(Story::from_url(url), expected, "{url}");
        }
    }
}
