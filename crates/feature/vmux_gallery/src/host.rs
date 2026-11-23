use bevy::prelude::{
    Add, App, Changed, Commands, Component, Entity, EntityEvent, IntoScheduleConfigs, On, Or,
    Plugin, Query, Update,
};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::{PageMetadata, UiState, UiStatePlugin, UiStateWrite};
use vmux_feature::manifest::FeaturePlugin;
use vmux_layout::hosted_page::HostedUiPlugin;

use crate::{
    ArgsPatch, Examples, GalleryAgentRequest, GalleryAttachRequest, GalleryBranchRequest,
    GalleryCheckedRequest, GalleryChoiceRequest, GalleryClickRequest, GalleryConfigureRequest,
    GalleryCursorRequest, GalleryDialogRequest, GalleryDisabledRequest, GalleryEffortRequest,
    GalleryMenuRequest, GalleryModelRequest, GalleryPasteRequest, GalleryPermissionRequest,
    GalleryPresetRequest, GalleryProgressRequest, GalleryProjectRequest,
    GalleryRemoveAttachmentRequest, GalleryResetRequest, GalleryStatusRequest, GalleryStopRequest,
    GallerySubmitRequest, GalleryTextRequest, GalleryUiState, Story, StoryDefinition,
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
            FeaturePlugin::<crate::Feature>::default(),
            HostedUiPlugin::<Preview>::new(Self::manifest()),
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
                GalleryConfigureRequest,
                GalleryPresetRequest,
            )>::default(),
            PreviewPlugin,
        ));
        #[cfg(ui)]
        app.add_plugins(crate::ui::GalleryPage::plugin());
    }
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
            .add_observer(configure)
            .add_observer(preset)
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
    button_variant: String,
    button_size: String,
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
            button_variant: "all".into(),
            button_size: "all".into(),
        }
    }
}

impl Preview {
    fn reset(&mut self, story: Story) {
        let revision = self.revision.wrapping_add(1);
        *self = Self::default();
        self.story = story;
        self.revision = revision;
        let defaults = self.definition().defaults;
        self.apply_args(&defaults);
    }

    fn record(&mut self, event: &str) {
        self.interaction_count = self.interaction_count.saturating_add(1);
        self.last_event = event.to_string();
    }

    fn definition(&self) -> StoryDefinition {
        ron::from_str(self.story.metadata_source())
            .expect("compiled story metadata must decode to typed args")
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
            presets: self.definition().presets,
            button_variant: self.button_variant.clone(),
            button_size: self.button_size.clone(),
        }
    }
}

impl Preview {
    fn address(&self, url: &str) -> PageAddress {
        let mut options = Vec::new();
        for story in Story::ALL {
            if story.category() != self.story.category() {
                continue;
            }
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
                    url: GalleryPlugin::url().into(),
                }),
                ..Default::default()
            },
            fixed: vec![
                AddressCrumb {
                    entry: AddressOption {
                        label_key: self.story.category().into(),
                        ..Default::default()
                    },
                    options,
                    ..Default::default()
                },
                AddressCrumb {
                    entry: AddressOption {
                        label: self.story.name().into(),
                        label_key: if self.story == Story::Icon {
                            "gallery-all-icons".into()
                        } else {
                            String::new()
                        },
                        intent: Some(AddressIntent::Navigate {
                            url: self.story.url(),
                        }),
                        ..Default::default()
                    },
                    active: true,
                    ..Default::default()
                },
            ],
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
                setup_url: None,
                account: None,
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
    preview.reset(story);
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

impl Preview {
    fn allows_args(&self, args: &ArgsPatch) -> bool {
        if args.progress.is_some_and(|value| !value.is_finite())
            || args.cursor.is_some_and(|value| value > 16)
        {
            return false;
        }
        for (value, allowed) in [
            (
                args.choice.as_deref(),
                &["one", "two", "three", "list", "grid"][..],
            ),
            (
                args.button_variant.as_deref(),
                &[
                    "all",
                    "primary",
                    "secondary",
                    "destructive",
                    "outline",
                    "ghost",
                ][..],
            ),
            (
                args.button_size.as_deref(),
                &["all", "xs", "sm", "md", "icon", "block"][..],
            ),
            (args.agent.as_deref(), &["codex", "claude"][..]),
            (args.model.as_deref(), &["balanced", "fast"][..]),
            (args.effort.as_deref(), &["", "low", "medium", "high"][..]),
            (args.permission.as_deref(), &["default", "plan"][..]),
            (
                args.project.as_deref(),
                &["/example/vmux", "/example/library"][..],
            ),
            (args.branch.as_deref(), &["main", "feature/ui"][..]),
            (
                args.status.as_deref(),
                &[
                    "idle",
                    "streaming",
                    "awaiting",
                    "installing",
                    "errored",
                    "in_use",
                ][..],
            ),
            (
                args.menu.as_deref(),
                &[
                    "",
                    "agent",
                    "model",
                    "effort",
                    "permission",
                    "project",
                    "branch",
                    "dropdown",
                    "dropdown-selector",
                    "breadcrumb",
                    "prompt",
                    "prompt-flat",
                    "mcp",
                    "project-select",
                    "manager-search",
                ][..],
            ),
        ] {
            if value.is_some_and(|value| !allowed.contains(&value)) {
                return false;
            }
        }
        true
    }

    fn apply_args(&mut self, args: &ArgsPatch) {
        if let Some(value) = args.disabled {
            self.disabled = value;
        }
        if let Some(value) = args.checked {
            self.checked = value;
        }
        if let Some(value) = &args.text {
            self.text = value.chars().take(4096).collect();
        }
        if let Some(value) = args.dialog_open {
            self.dialog_open = value;
        }
        if let Some(value) = args.progress {
            self.progress = value.clamp(0.0, 100.0);
        }
        if let Some(value) = &args.menu {
            self.menu = value.clone();
        }
        if let Some(value) = args.cursor {
            self.cursor = value;
        }
        if let Some(value) = &args.agent {
            self.agent = value.clone();
        }
        if let Some(value) = &args.model {
            self.model = value.clone();
        }
        if let Some(value) = &args.effort {
            self.effort = value.clone();
        }
        if let Some(value) = &args.permission {
            self.permission = value.clone();
        }
        if let Some(value) = &args.project {
            self.project = value.clone();
        }
        if let Some(value) = &args.branch {
            self.branch = value.clone();
        }
        if let Some(value) = args.running {
            self.running = value;
        }
        if let Some(value) = args.has_attachment {
            self.has_attachment = value;
        }
        if let Some(value) = &args.status {
            self.status = value.clone();
        }
        if let Some(value) = &args.choice {
            self.choice = value.clone();
        }
        if let Some(value) = &args.button_variant {
            self.button_variant = value.clone();
        }
        if let Some(value) = &args.button_size {
            self.button_size = value.clone();
        }
    }
}

fn configure(event: On<UiInput<GalleryConfigureRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if preview.revision != event.payload.revision || !preview.allows_args(&event.payload.args) {
        return;
    }
    preview.apply_args(&event.payload.args);
    preview.record("GalleryConfigureRequest");
}

fn preset(event: On<UiInput<GalleryPresetRequest>>, mut previews: Query<&mut Preview>) {
    let Ok(mut preview) = previews.get_mut(event.webview) else {
        return;
    };
    if preview.revision != event.payload.revision {
        return;
    }
    let definition = preview.definition();
    let Some(selected) = definition.presets.get(event.payload.index) else {
        return;
    };
    if !preview.allows_args(&selected.args) {
        return;
    }
    let story = preview.story;
    preview.reset(story);
    preview.apply_args(&selected.args);
    preview.record("GalleryPresetRequest");
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
    fn registered_ron_definitions_decode_and_accept_every_named_preset() {
        let mut app = App::new();
        app.add_plugins(PreviewPlugin);
        for story in Story::ALL {
            let page = app
                .world_mut()
                .spawn((
                    PageMetadata {
                        url: story.url(),
                        ..Default::default()
                    },
                    Preview::default(),
                ))
                .id();
            let preview = app.world().get::<Preview>(page).unwrap();
            let definition = preview.definition();
            assert!(
                preview.allows_args(&definition.defaults),
                "{}",
                story.name()
            );
            for (index, preset) in definition.presets.iter().enumerate() {
                let revision = app.world().get::<Preview>(page).unwrap().revision;
                app.world_mut().trigger(UiInput {
                    webview: page,
                    payload: GalleryPresetRequest { revision, index },
                });
                app.update();
                let preview = app.world().get::<Preview>(page).unwrap();
                assert_eq!(
                    preview.revision,
                    revision + 1,
                    "rejected RON preset: {} / {}",
                    story.name(),
                    preset.name,
                );
            }
            app.world_mut().despawn(page);
        }
    }

    #[test]
    fn colocated_ron_presets_apply_typed_conditions_and_reset_prior_args() {
        let mut app = App::new();
        app.add_plugins(PreviewPlugin);
        let page = app.world_mut().spawn(Preview::default()).id();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryConfigureRequest {
                revision: 0,
                args: ArgsPatch {
                    text: Some("Prior content".into()),
                    ..Default::default()
                },
            },
        });
        let definition = app.world().get::<Preview>(page).unwrap().definition();
        let index = definition
            .presets
            .iter()
            .position(|preset| preset.name == "DisabledOutline")
            .expect("required Button preset");
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryPresetRequest { revision: 0, index },
        });
        app.update();
        let preview = app.world().get::<Preview>(page).unwrap();
        assert!(preview.disabled);
        assert_eq!(preview.text, "Disabled preview");
        assert_eq!(preview.button_variant, "outline");
        assert_eq!(preview.button_size, "md");
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryPresetRequest {
                revision: 1,
                index: 0,
            },
        });
        app.update();
        let preview = app.world().get::<Preview>(page).unwrap();
        assert!(!preview.disabled);
        assert!(preview.text.is_empty());
    }

    #[test]
    fn controls_change_multiple_conditions_while_the_preview_is_disabled() {
        let mut app = App::new();
        app.add_plugins(PreviewPlugin);
        let page = app.world_mut().spawn(Preview::default()).id();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryConfigureRequest {
                revision: 0,
                args: ArgsPatch {
                    disabled: Some(true),
                    text: Some("Disabled preview".into()),
                    model: Some("fast".into()),
                    has_attachment: Some(true),
                    ..Default::default()
                },
            },
        });
        app.update();
        let preview = app.world().get::<Preview>(page).unwrap();
        assert!(preview.disabled && preview.has_attachment);
        assert_eq!(preview.text, "Disabled preview");
        assert_eq!(preview.model, "fast");
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryTextRequest {
                revision: 0,
                text: "interactive input".into(),
            },
        });
        app.update();
        assert_eq!(
            app.world().get::<Preview>(page).unwrap().text,
            "Disabled preview"
        );
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryConfigureRequest {
                revision: 0,
                args: ArgsPatch {
                    text: Some("should not apply".into()),
                    model: Some("unknown".into()),
                    ..Default::default()
                },
            },
        });
        app.update();
        assert_eq!(
            app.world().get::<Preview>(page).unwrap().text,
            "Disabled preview"
        );
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GalleryConfigureRequest {
                revision: 0,
                args: ArgsPatch {
                    text: Some("Controls still work".into()),
                    ..Default::default()
                },
            },
        });
        app.update();
        assert_eq!(
            app.world().get::<Preview>(page).unwrap().text,
            "Controls still work"
        );
    }

    #[test]
    fn every_gallery_link_is_claimed_by_the_registered_host() {
        let mut app = App::new();
        app.add_plugins(GalleryPlugin);
        let mut registered = app.world_mut().query::<&vmux_ecs::page::HostedPage>();
        for story in Story::ALL {
            let url = story.url();
            assert!(
                registered
                    .iter(app.world())
                    .any(|page| page.answers_for(&url)),
                "unregistered gallery link: {url}",
            );
        }
        assert!(
            !registered
                .iter(app.world())
                .any(|page| page.answers_for("vmux://uis/favicon")),
        );
    }

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
        assert_eq!(
            address.fixed[0].entry.label_key,
            "gallery-category-composition"
        );
        assert_eq!(address.fixed[1].entry.label, "PromptComposer");
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
