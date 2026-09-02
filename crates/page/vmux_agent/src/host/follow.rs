use std::path::{Path, PathBuf};

use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_command::WriteAppCommands;
use vmux_core::agent::AgentKind;
use vmux_core::event::{ExplorerSearchFile, ExplorerSearchMatch};
use vmux_layout::pane::Pane;
use vmux_service::protocol::AgentCommand as ServiceAgentCommand;
use vmux_setting::AppSettings;
use vmux_terminal::ServiceMessageSet;

use crate::events::{AgentCommandRequest, CommandOrigin};
use crate::session::AgentSession;

use super::command::origin_is_agent;

pub(super) struct FollowPlugin;

impl Plugin for FollowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                handle_agent_file_touch.before(vmux_layout::worktree::TabDirectoryRebindSet),
                handle_agent_file_search,
            )
                .chain()
                .in_set(WriteAppCommands)
                .after(ServiceMessageSet)
                .after(super::command::handle_agent_commands),
        )
        .add_systems(
            Update,
            tidy_on_agent_attention
                .after(vmux_layout::stack::ComputeFocusSet)
                .after(super::attention::handle_agent_turn_ended),
        )
        .add_systems(
            Update,
            (tidy_acp_on_idle, tidy_page_on_idle).after(vmux_layout::stack::ComputeFocusSet),
        );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AgentFileResolve<'w, 's> {
    activate: MessageWriter<'w, vmux_layout::active_panes::ActivatePane>,
    page_open: MessageWriter<'w, vmux_core::PageOpenRequest>,
    open_beside: MessageWriter<'w, vmux_layout::OpenBesideRequest>,
    observations: MessageWriter<'w, vmux_layout::worktree::TabDirectoryObserved>,
    agent_terms: Query<
        'w,
        's,
        (
            Entity,
            &'static vmux_service::protocol::ProcessId,
            &'static ChildOf,
        ),
    >,
    kinds: Query<'w, 's, &'static AgentSession>,
    child_of: Query<'w, 's, &'static ChildOf>,
    file_pages: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static vmux_core::PageMetadata,
            Option<&'static vmux_git::GitDiffSource>,
        ),
    >,
    pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    stack_q: Query<'w, 's, Entity, With<vmux_layout::stack::Stack>>,
    tabs: Query<'w, 's, (), With<vmux_layout::tab::Tab>>,
}

#[derive(Clone, Copy)]
pub(crate) struct FilePageTarget {
    stack: Entity,
    pane: Entity,
    navigate: bool,
}

pub(crate) struct PendingFilePreview {
    anchor: vmux_service::protocol::ProcessId,
    agent_pane: Entity,
    url: String,
    request_id: [u8; 16],
    user_origin: bool,
    kind: vmux_service::protocol::FileTouchKind,
}

impl AgentFileResolve<'_, '_> {
    fn agent_pane(&self, anchor: vmux_service::protocol::ProcessId) -> Option<Entity> {
        let (_, _, term_co) = self
            .agent_terms
            .iter()
            .find(|(_, pid, _)| **pid == anchor)?;
        self.child_of.get(term_co.get()).ok().map(|co| co.get())
    }

    fn agent_kind(&self, anchor: vmux_service::protocol::ProcessId) -> Option<AgentKind> {
        let (entity, _, _) = self
            .agent_terms
            .iter()
            .find(|(_, pid, _)| **pid == anchor)?;
        self.kinds.get(entity).ok().map(|session| session.kind)
    }

    fn ancestor_tab(&self, entity: Entity) -> Option<Entity> {
        let mut current = entity;
        loop {
            if self.tabs.contains(current) {
                return Some(current);
            }
            current = self.child_of.get(current).ok()?.get();
        }
    }

    fn stack_has_file_page(&self, stack: Entity) -> bool {
        self.file_pages.iter().any(|(_, child_of, metadata, _)| {
            child_of.get() == stack && metadata.url.starts_with("file:")
        })
    }

    fn pane_has_only_file_stacks(&self, pane: Entity) -> bool {
        let Ok(children) = self.pane_children.get(pane) else {
            return false;
        };
        let mut found = false;
        for stack in children
            .iter()
            .filter(|stack| self.stack_q.contains(*stack))
        {
            found = true;
            if !self.stack_has_file_page(stack) {
                return false;
            }
        }
        found
    }

    fn file_panes_for(&self, agent_pane: Entity) -> Vec<Entity> {
        let Some(agent_tab) = self.ancestor_tab(agent_pane) else {
            return Vec::new();
        };
        let agent_parent = self.child_of.get(agent_pane).ok().map(Relationship::get);
        let mut panes = Vec::new();
        for (_, page_child, metadata, _) in self.file_pages.iter() {
            if !metadata.url.starts_with("file:") {
                continue;
            }
            let stack = page_child.get();
            let Ok(pane_child) = self.child_of.get(stack) else {
                continue;
            };
            let pane = pane_child.get();
            if pane == agent_pane
                || self.ancestor_tab(pane) != Some(agent_tab)
                || !self.pane_has_only_file_stacks(pane)
                || panes.contains(&pane)
            {
                continue;
            }
            panes.push(pane);
        }
        panes.sort_by_key(|pane| {
            let direct = self.child_of.get(*pane).ok().map(Relationship::get) == agent_parent;
            !direct
        });
        panes
    }

    fn file_page_for(&self, agent_pane: Entity) -> Option<(Entity, Entity)> {
        let pane = self.file_panes_for(agent_pane).into_iter().next()?;
        for (page, page_co, meta, _) in self.file_pages.iter() {
            if !meta.url.starts_with("file:") {
                continue;
            }
            let Ok(pane_co) = self.child_of.get(page_co.get()) else {
                continue;
            };
            if pane_co.get() == pane {
                return Some((page, pane));
            }
        }
        None
    }

    fn file_page_target(&self, agent_pane: Entity, url: &str) -> Option<FilePageTarget> {
        let panes = self.file_panes_for(agent_pane);
        for pane in &panes {
            for (_, page_co, meta, diff) in self.file_pages.iter() {
                let stack = page_co.get();
                if !meta.url.starts_with("file:")
                    || self.child_of.get(stack).ok().map(Relationship::get) != Some(*pane)
                    || !vmux_layout::placement::reusable_page_match(url, &meta.url)
                {
                    continue;
                }
                let dirty = diff.is_some_and(|source| source.dirty);
                return Some(FilePageTarget {
                    stack,
                    pane: *pane,
                    navigate: !dirty && meta.url != url,
                });
            }
        }
        for pane in panes {
            for (_, page_co, meta, diff) in self.file_pages.iter() {
                let stack = page_co.get();
                if !meta.url.starts_with("file:")
                    || self.child_of.get(stack).ok().map(Relationship::get) != Some(pane)
                    || diff.is_some_and(|source| source.dirty)
                {
                    continue;
                }
                return Some(FilePageTarget {
                    stack,
                    pane,
                    navigate: true,
                });
            }
        }
        None
    }

    #[allow(clippy::type_complexity)]
    fn file_stacks_for(
        &self,
        agent_pane: Entity,
    ) -> Option<(Entity, Vec<(Entity, Entity, String)>)> {
        let follow_pane = self.file_panes_for(agent_pane).into_iter().next()?;
        let mut stacks = Vec::new();
        for (page, page_co, meta, _) in self.file_pages.iter() {
            if !meta.url.starts_with("file:") {
                continue;
            }
            let stack = page_co.get();
            let Ok(pane_co) = self.child_of.get(stack) else {
                continue;
            };
            let pane = pane_co.get();
            if pane != follow_pane {
                continue;
            }
            stacks.push((stack, page, meta.url.clone()));
        }
        Some((follow_pane, stacks))
    }
}

pub(crate) fn file_touch_url(
    path: &str,
    line: Option<u32>,
    col: Option<u32>,
    end_col: Option<u32>,
) -> String {
    let mut url = url::Url::from_file_path(path)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{path}"));
    if let Some(l) = line {
        url.push_str(&format!("#L{l}"));
        if let (Some(c), Some(e)) = (col, end_col) {
            url.push_str(&format!(":{c}-{e}"));
        }
    }
    url
}

fn handle_agent_file_touch(
    mut reader: MessageReader<AgentCommandRequest>,
    mut resolve: AgentFileResolve,
    settings: Res<AppSettings>,
    mut file_view_mode: Option<ResMut<Messages<vmux_editor::FileViewModeRequest>>>,
) {
    let mut previews: std::collections::HashMap<Entity, Vec<PendingFilePreview>> =
        std::collections::HashMap::new();
    let mut request_diff_mode = false;
    for request in reader.read() {
        let ServiceAgentCommand::FileTouched {
            anchor,
            path,
            line,
            col,
            end_col,
            kind,
        } = &request.command
        else {
            continue;
        };
        if let CommandOrigin::Agent {
            anchor: Some(origin_anchor),
            ..
        } = &request.origin
            && origin_anchor != anchor
        {
            continue;
        }
        if *kind == vmux_service::protocol::FileTouchKind::Read
            && Path::new(path).file_name().and_then(|name| name.to_str()) == Some("SKILL.md")
        {
            continue;
        }
        let Some(agent_pane) = resolve.agent_pane(*anchor) else {
            continue;
        };
        if let Some(tab) = resolve.ancestor_tab(agent_pane) {
            let kind = match kind {
                vmux_service::protocol::FileTouchKind::Read => {
                    vmux_layout::worktree::TabDirectoryObservationKind::Read
                }
                vmux_service::protocol::FileTouchKind::Edit => {
                    vmux_layout::worktree::TabDirectoryObservationKind::Edit
                }
            };
            resolve
                .observations
                .write(vmux_layout::worktree::TabDirectoryObserved {
                    tab,
                    path: PathBuf::from(path),
                    kind,
                });
        }
        if !settings.agent.follow_files {
            continue;
        }
        request_diff_mode |= *kind == vmux_service::protocol::FileTouchKind::Edit;
        previews
            .entry(agent_pane)
            .or_default()
            .push(PendingFilePreview {
                anchor: *anchor,
                agent_pane,
                url: file_touch_url(path, *line, *col, *end_col),
                request_id: request.request_id.0,
                user_origin: !origin_is_agent(&request.origin),
                kind: *kind,
            });
    }
    if request_diff_mode && let Some(file_view_mode) = file_view_mode.as_mut() {
        file_view_mode.write(vmux_editor::FileViewModeRequest(
            vmux_core::event::FileViewMode::Diff,
        ));
    }
    for previews in previews.into_values() {
        let all_reads = previews
            .iter()
            .all(|preview| preview.kind == vmux_service::protocol::FileTouchKind::Read);
        let deduped = if all_reads {
            previews.into_iter().last().into_iter().collect()
        } else {
            let mut deduped: Vec<PendingFilePreview> = Vec::new();
            for preview in previews {
                if let Some(existing) = deduped.iter_mut().find(|existing| {
                    vmux_layout::placement::reusable_page_match(&preview.url, &existing.url)
                }) {
                    *existing = preview;
                } else {
                    deduped.push(preview);
                }
            }
            deduped
        };
        let open_as_tabs = deduped.len() > 1;
        for preview in deduped {
            let anchor = preview.anchor;
            let existing = resolve.file_page_for(preview.agent_pane);
            let target = (!open_as_tabs)
                .then(|| resolve.file_page_target(preview.agent_pane, &preview.url))
                .flatten();
            if let Some(target) = target {
                if target.navigate {
                    resolve.page_open.write(vmux_core::PageOpenRequest {
                        target: vmux_core::PageOpenTarget::Stack(target.stack),
                        url: preview.url,
                        request_id: None,
                    });
                }
            } else {
                resolve.open_beside.write(vmux_layout::OpenBesideRequest {
                    pane: preview.agent_pane,
                    direction: None,
                    url: preview.url,
                    request_id: preview.request_id,
                    focus: preview.user_origin && existing.is_some(),
                });
            }
            if let Some(pane) = target
                .map(|target| target.pane)
                .or(existing.map(|(_, pane)| pane))
            {
                let kind = resolve.agent_kind(anchor);
                resolve
                    .activate
                    .write(vmux_layout::active_panes::ActivatePane {
                        profile: vmux_layout::active_panes::ProfileId::Agent(format!("{anchor:?}")),
                        active: vmux_layout::active_panes::ActiveStack {
                            tab: None,
                            pane: Some(pane),
                            stack: None,
                            kind,
                        },
                    });
            }
        }
    }
}

fn handle_agent_file_search(
    mut reader: MessageReader<AgentCommandRequest>,
    mut writer: MessageWriter<vmux_editor::GlobalSearchRequest>,
) {
    for request in reader.read() {
        let ServiceAgentCommand::FileSearch {
            root,
            query,
            matches,
            ..
        } = &request.command
        else {
            continue;
        };
        let files = SearchGrouping::of(matches);
        let Some(first) = files.first() else {
            continue;
        };
        writer.write(vmux_editor::GlobalSearchRequest {
            target_path: PathBuf::from(&first.path),
            root: root.clone(),
            query: query.clone(),
            files,
            capped: false,
        });
    }
}

struct SearchGrouping;

impl SearchGrouping {
    fn of(matches: &[vmux_wire::protocol::FileSearchMatch]) -> Vec<ExplorerSearchFile> {
        let mut files: Vec<ExplorerSearchFile> = Vec::new();
        for result in matches {
            let hit = ExplorerSearchMatch {
                line: result.line,
                col: result.col,
                end_col: result.end_col,
                preview: result.preview.clone(),
            };
            if let Some(file) = files.iter_mut().find(|file| file.path == result.path) {
                file.matches.push(hit);
                continue;
            }
            files.push(ExplorerSearchFile {
                path: result.path.clone(),
                matches: vec![hit],
                capped: false,
            });
        }
        files
    }
}

#[allow(clippy::too_many_arguments)]
fn tidy_follow_pane(
    agent_pane: Entity,
    settings: &AppSettings,
    resolve: &AgentFileResolve,
    last_activated: &Query<&vmux_core::LastActivatedAt>,
    pending: &Query<(), With<crate::tidy::PendingTidy>>,
    close: &mut MessageWriter<vmux_layout::CloseStackRequest>,
    commands: &mut Commands,
) {
    let Some((follow_pane, stacks)) = resolve.file_stacks_for(agent_pane) else {
        return;
    };
    if pending.get(follow_pane).is_ok() {
        return;
    }
    let mut repos: Vec<(std::path::PathBuf, std::collections::HashSet<String>)> = Vec::new();
    let rows: Vec<(Entity, i64, bool)> = stacks
        .iter()
        .map(|(stack, _page, url)| {
            let ts = last_activated.get(*stack).map(|t| t.0).unwrap_or(i64::MIN);
            let changed = crate::tidy::path_from_file_url(url)
                .map(|abs| crate::tidy::is_changed(&abs, &mut repos))
                .unwrap_or(false);
            (*stack, ts, changed)
        })
        .collect();
    let closable = crate::tidy::decide_closable(&rows, settings.agent.tidy_files_max);
    if closable.is_empty() {
        return;
    }
    if settings.agent.tidy_files_auto {
        for stack in closable {
            close.write(vmux_layout::CloseStackRequest::tidying(stack));
        }
        return;
    }
    let count = closable.len() as u32;
    let active_page = stacks
        .iter()
        .max_by_key(|(stack, _, _)| last_activated.get(*stack).map(|t| t.0).unwrap_or(i64::MIN))
        .map(|(_, page, _)| *page);
    if let Some(page) = active_page {
        commands.trigger(bevy_cef::prelude::BinHostEmitEvent::from_rkyv(
            page,
            vmux_core::event::FILE_TIDY_PROMPT_EVENT,
            &vmux_core::event::FileTidyPromptEvent { count },
        ));
        commands
            .entity(follow_pane)
            .insert(crate::tidy::PendingTidy { closable });
    }
}

pub(super) fn tidy_on_agent_attention(
    mut reader: MessageReader<vmux_core::notify::AgentAttention>,
    settings: Res<AppSettings>,
    agents: Query<&vmux_service::protocol::ProcessId, With<vmux_core::team::Agent>>,
    resolve: AgentFileResolve,
    last_activated: Query<&vmux_core::LastActivatedAt>,
    pending: Query<(), With<crate::tidy::PendingTidy>>,
    mut close: MessageWriter<vmux_layout::CloseStackRequest>,
    mut commands: Commands,
) {
    if !settings.agent.tidy_files {
        for _ in reader.read() {}
        return;
    }
    for att in reader.read() {
        let Ok(pid) = agents.get(att.entity) else {
            continue;
        };
        let Some(agent_pane) = resolve.agent_pane(*pid) else {
            continue;
        };
        tidy_follow_pane(
            agent_pane,
            &settings,
            &resolve,
            &last_activated,
            &pending,
            &mut close,
            &mut commands,
        );
    }
}

fn tidy_acp_on_idle(
    settings: Res<AppSettings>,
    sessions: Query<
        (&vmux_session::AcpSession, &crate::AgentRunState),
        Changed<crate::AgentRunState>,
    >,
    resolve: AgentFileResolve,
    last_activated: Query<&vmux_core::LastActivatedAt>,
    pending: Query<(), With<crate::tidy::PendingTidy>>,
    mut close: MessageWriter<vmux_layout::CloseStackRequest>,
    mut commands: Commands,
) {
    if !settings.agent.tidy_files {
        return;
    }
    for (acp, state) in &sessions {
        if !matches!(state, crate::AgentRunState::Idle) {
            continue;
        }
        let Some(agent_pane) = resolve.agent_pane(acp.anchor) else {
            continue;
        };
        tidy_follow_pane(
            agent_pane,
            &settings,
            &resolve,
            &last_activated,
            &pending,
            &mut close,
            &mut commands,
        );
    }
}

fn tidy_page_on_idle(
    settings: Res<AppSettings>,
    sessions: Query<
        (&ChildOf, &crate::AgentRunState),
        (
            With<vmux_session::AgentSession>,
            Changed<crate::AgentRunState>,
        ),
    >,
    resolve: AgentFileResolve,
    last_activated: Query<&vmux_core::LastActivatedAt>,
    pending: Query<(), With<crate::tidy::PendingTidy>>,
    mut close: MessageWriter<vmux_layout::CloseStackRequest>,
    mut commands: Commands,
) {
    if !settings.agent.tidy_files {
        return;
    }
    for (parent, state) in &sessions {
        if !matches!(state, crate::AgentRunState::Idle) {
            continue;
        }
        tidy_follow_pane(
            parent.get(),
            &settings,
            &resolve,
            &last_activated,
            &pending,
            &mut close,
            &mut commands,
        );
    }
}

pub(crate) fn on_tidy_action(
    trigger: On<bevy_cef::prelude::BinReceive<vmux_core::event::FileTidyActionEvent>>,
    child_of: Query<&ChildOf>,
    pending: Query<&crate::tidy::PendingTidy>,
    mut settings: ResMut<AppSettings>,
    mut save: MessageWriter<vmux_setting::SettingsSaveRequest>,
    mut close: MessageWriter<vmux_layout::CloseStackRequest>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(stack) = child_of.get(webview).map(Relationship::get) else {
        return;
    };
    let Ok(pane) = child_of.get(stack).map(Relationship::get) else {
        return;
    };
    let Ok(pending_tidy) = pending.get(pane) else {
        return;
    };
    let closable = pending_tidy.closable.clone();
    commands.entity(pane).remove::<crate::tidy::PendingTidy>();
    match trigger.event().payload.choice {
        vmux_core::event::TidyChoice::Dismiss => {}
        vmux_core::event::TidyChoice::Always => {
            settings.agent.tidy_files_auto = true;
            save.write(vmux_setting::SettingsSaveRequest);
            for stack in closable {
                close.write(vmux_layout::CloseStackRequest::tidying(stack));
            }
        }
        vmux_core::event::TidyChoice::Tidy => {
            for stack in closable {
                close.write(vmux_layout::CloseStackRequest::tidying(stack));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::run_terminal::AgentCwd;
    use crate::host::test_support::{
        close_stack_requests, spawn_file_preview_stack, test_settings,
    };
    use vmux_layout::pane::PaneSplit;
    use vmux_service::protocol::{AgentRequestId, ProcessId};

    #[test]
    pub(crate) fn file_touch_url_builds_goto_fragment() {
        assert_eq!(
            file_touch_url("/a/b.rs", None, None, None),
            "file:///a/b.rs"
        );
        assert_eq!(
            file_touch_url("/a/b.rs", Some(10), None, None),
            "file:///a/b.rs#L10"
        );
        assert_eq!(
            file_touch_url("/a/b.rs", Some(10), Some(5), Some(12)),
            "file:///a/b.rs#L10:5-12"
        );
    }

    pub(crate) fn file_touch_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_layout::LayoutContractPlugin,
            vmux_editor::EditorContractPlugin,
        ))
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_core::PageOpenRequest>()
        .insert_resource(test_settings())
        .add_systems(Update, handle_agent_file_touch);
        app
    }

    pub(crate) fn spawn_file_touch_layout(
        app: &mut App,
        old_url: &str,
        dirty: bool,
    ) -> (ProcessId, Entity) {
        let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
        let agent_pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let agent_stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(agent_pane)))
            .id();
        let anchor = ProcessId::new();
        app.world_mut().spawn((anchor, ChildOf(agent_stack)));
        let file_pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let file_stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(file_pane)))
            .id();
        app.world_mut().spawn((
            vmux_core::PageMetadata {
                url: old_url.to_string(),
                ..default()
            },
            vmux_git::GitDiffSource { dirty, ..default() },
            ChildOf(file_stack),
        ));
        (anchor, file_stack)
    }

    pub(crate) fn send_file_touch(
        app: &mut App,
        anchor: ProcessId,
        path: &str,
        kind: vmux_service::protocol::FileTouchKind,
    ) {
        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                command: ServiceAgentCommand::FileTouched {
                    anchor,
                    path: path.to_string(),
                    line: None,
                    col: None,
                    end_col: None,
                    kind,
                },
            });
    }

    pub(crate) fn send_file_read(app: &mut App, anchor: ProcessId, path: &str) {
        send_file_touch(
            app,
            anchor,
            path,
            vmux_service::protocol::FileTouchKind::Read,
        );
    }

    pub(crate) fn send_file_edit(app: &mut App, anchor: ProcessId, path: &str) {
        send_file_touch(
            app,
            anchor,
            path,
            vmux_service::protocol::FileTouchKind::Edit,
        );
    }

    #[test]
    pub(crate) fn file_read_replaces_clean_follow_stack() {
        let mut app = file_touch_test_app();
        let (anchor, file_stack) = spawn_file_touch_layout(&mut app, "file:///repo/old.rs", false);
        send_file_read(&mut app, anchor, "/repo/new.rs");

        app.update();

        let opens: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_core::PageOpenRequest>>()
            .drain()
            .collect();
        assert_eq!(opens.len(), 1);
        assert!(matches!(
            opens[0].target,
            vmux_core::PageOpenTarget::Stack(stack) if stack == file_stack
        ));
        assert_eq!(opens[0].url, "file:///repo/new.rs");
        let beside = app
            .world_mut()
            .resource_mut::<Messages<vmux_layout::OpenBesideRequest>>()
            .drain()
            .count();
        assert_eq!(beside, 0);
    }

    #[test]
    pub(crate) fn file_read_replaces_clean_follow_stack_across_nested_split() {
        let mut app = file_touch_test_app();
        let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
        let root = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: vmux_layout::pane::PaneSplitDirection::Row,
                },
                ChildOf(tab),
            ))
            .id();
        let agent_pane = app.world_mut().spawn((Pane, ChildOf(root))).id();
        let agent_stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(agent_pane)))
            .id();
        let anchor = ProcessId::new();
        app.world_mut().spawn((anchor, ChildOf(agent_stack)));
        let nested = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit {
                    direction: vmux_layout::pane::PaneSplitDirection::Column,
                },
                ChildOf(root),
            ))
            .id();
        app.world_mut().spawn((Pane, ChildOf(nested)));
        let file_pane = app.world_mut().spawn((Pane, ChildOf(nested))).id();
        let file_stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(file_pane)))
            .id();
        app.world_mut().spawn((
            vmux_core::PageMetadata {
                url: "file:///repo/old.rs".into(),
                ..default()
            },
            vmux_git::GitDiffSource::default(),
            ChildOf(file_stack),
        ));
        send_file_read(&mut app, anchor, "/repo/new.rs");

        app.update();

        let opens: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_core::PageOpenRequest>>()
            .drain()
            .collect();
        assert_eq!(opens.len(), 1);
        assert!(matches!(
            opens[0].target,
            vmux_core::PageOpenTarget::Stack(stack) if stack == file_stack
        ));
        assert_eq!(opens[0].url, "file:///repo/new.rs");
    }

    #[test]
    pub(crate) fn file_search_forwards_results_to_editor() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_editor::EditorContractPlugin))
            .add_message::<AgentCommandRequest>()
            .add_systems(Update, handle_agent_file_search);
        let anchor = ProcessId::new();
        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                command: ServiceAgentCommand::FileSearch {
                    anchor,
                    root: "/repo".into(),
                    query: "needle".into(),
                    matches: vec![
                        vmux_service::protocol::FileSearchMatch {
                            path: "/repo/src/main.rs".into(),
                            line: 9,
                            col: 4,
                            end_col: 10,
                            preview: "let needle = true;".into(),
                        },
                        vmux_service::protocol::FileSearchMatch {
                            path: "/repo/src/lib.rs".into(),
                            line: 2,
                            col: 0,
                            end_col: 6,
                            preview: "needle".into(),
                        },
                        vmux_service::protocol::FileSearchMatch {
                            path: "/repo/src/main.rs".into(),
                            line: 21,
                            col: 8,
                            end_col: 14,
                            preview: "    needle();".into(),
                        },
                    ],
                },
            });

        app.update();

        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_editor::GlobalSearchRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target_path, PathBuf::from("/repo/src/main.rs"));
        assert_eq!(requests[0].query, "needle");
        let files = &requests[0].files;
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "/repo/src/main.rs");
        assert_eq!(files[1].path, "/repo/src/lib.rs");
        let lines: Vec<u32> = files[0].matches.iter().map(|hit| hit.line).collect();
        assert_eq!(lines, vec![9, 21]);
        assert_eq!(files[1].matches.len(), 1);
    }

    #[test]
    pub(crate) fn same_frame_file_reads_replace_once_with_last_touch() {
        let mut app = file_touch_test_app();
        let (anchor, file_stack) = spawn_file_touch_layout(&mut app, "file:///repo/old.rs", false);
        send_file_read(&mut app, anchor, "/repo/first.rs");
        send_file_read(&mut app, anchor, "/repo/second.rs");
        send_file_read(&mut app, anchor, "/repo/first.rs");

        app.update();

        let opens: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_core::PageOpenRequest>>()
            .drain()
            .collect();
        assert_eq!(opens.len(), 1);
        assert!(matches!(
            opens[0].target,
            vmux_core::PageOpenTarget::Stack(stack) if stack == file_stack
        ));
        assert_eq!(opens[0].url, "file:///repo/first.rs");
        let view_modes = app
            .world_mut()
            .resource_mut::<Messages<vmux_editor::FileViewModeRequest>>()
            .drain()
            .count();
        assert_eq!(view_modes, 0);
    }

    #[test]
    pub(crate) fn same_frame_file_edits_open_each_distinct_file_as_tabs() {
        let mut app = file_touch_test_app();
        let (anchor, _) = spawn_file_touch_layout(&mut app, "file:///repo/old.rs", false);
        send_file_edit(&mut app, anchor, "/repo/first.rs");
        send_file_edit(&mut app, anchor, "/repo/second.rs");
        send_file_edit(&mut app, anchor, "/repo/first.rs");

        app.update();

        let opens = app
            .world_mut()
            .resource_mut::<Messages<vmux_core::PageOpenRequest>>()
            .drain()
            .count();
        assert_eq!(opens, 0);
        let beside: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_layout::OpenBesideRequest>>()
            .drain()
            .collect();
        assert_eq!(beside.len(), 2);
        assert_eq!(beside[0].url, "file:///repo/first.rs");
        assert_eq!(beside[1].url, "file:///repo/second.rs");
        let view_modes: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_editor::FileViewModeRequest>>()
            .drain()
            .collect();
        assert_eq!(
            view_modes,
            vec![vmux_editor::FileViewModeRequest(
                vmux_core::event::FileViewMode::Diff
            )]
        );
    }

    #[test]
    pub(crate) fn file_read_preserves_dirty_follow_stack() {
        let mut app = file_touch_test_app();
        let (anchor, _) = spawn_file_touch_layout(&mut app, "file:///repo/old.rs", true);
        send_file_read(&mut app, anchor, "/repo/new.rs");

        app.update();

        let opens = app
            .world_mut()
            .resource_mut::<Messages<vmux_core::PageOpenRequest>>()
            .drain()
            .count();
        assert_eq!(opens, 0);
        let beside: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<vmux_layout::OpenBesideRequest>>()
            .drain()
            .collect();
        assert_eq!(beside.len(), 1);
        assert_eq!(beside[0].url, "file:///repo/new.rs");
    }

    #[test]
    pub(crate) fn file_read_does_not_reload_matching_dirty_page() {
        let mut app = file_touch_test_app();
        let (anchor, _) = spawn_file_touch_layout(&mut app, "file:///repo/current.rs", true);
        send_file_read(&mut app, anchor, "/repo/current.rs");

        app.update();

        let opens = app
            .world_mut()
            .resource_mut::<Messages<vmux_core::PageOpenRequest>>()
            .drain()
            .count();
        let beside = app
            .world_mut()
            .resource_mut::<Messages<vmux_layout::OpenBesideRequest>>()
            .drain()
            .count();
        assert_eq!((opens, beside), (0, 0));
    }

    #[test]
    pub(crate) fn skill_file_read_does_not_open_follow_pane() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_layout::LayoutContractPlugin,
            vmux_editor::EditorContractPlugin,
        ))
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_core::PageOpenRequest>()
        .insert_resource(test_settings())
        .add_systems(Update, handle_agent_file_touch);

        let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
            .id();
        let anchor = ProcessId::new();
        app.world_mut().spawn((anchor, ChildOf(stack)));

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                command: ServiceAgentCommand::FileTouched {
                    anchor,
                    path: "/Users/me/.agents/skills/caveman/SKILL.md".into(),
                    line: None,
                    col: None,
                    end_col: None,
                    kind: vmux_service::protocol::FileTouchKind::Read,
                },
            });

        app.update();

        let previews = app
            .world()
            .resource::<Messages<vmux_layout::OpenBesideRequest>>();
        let mut preview_cursor = previews.get_cursor();
        assert_eq!(preview_cursor.read(previews).count(), 0);
        let observations = app
            .world()
            .resource::<Messages<vmux_layout::worktree::TabDirectoryObserved>>();
        let mut observation_cursor = observations.get_cursor();
        assert_eq!(observation_cursor.read(observations).count(), 0);
    }

    #[test]
    pub(crate) fn file_touch_emits_tab_directory_observation_when_file_follow_is_disabled() {
        let mut settings = test_settings();
        settings.agent.follow_files = false;
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_layout::LayoutContractPlugin,
            vmux_editor::EditorContractPlugin,
        ))
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_core::PageOpenRequest>()
        .insert_resource(settings)
        .add_systems(Update, handle_agent_file_touch);

        let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
            .id();
        let anchor = ProcessId::new();
        app.world_mut().spawn((anchor, ChildOf(stack)));
        let path = std::env::temp_dir().join("vmux-observed-file.rs");

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                command: ServiceAgentCommand::FileTouched {
                    anchor,
                    path: path.to_string_lossy().into_owned(),
                    line: None,
                    col: None,
                    end_col: None,
                    kind: vmux_service::protocol::FileTouchKind::Read,
                },
            });

        app.update();

        let messages = app
            .world()
            .resource::<Messages<vmux_layout::worktree::TabDirectoryObserved>>();
        let mut cursor = messages.get_cursor();
        let observations: Vec<_> = cursor.read(messages).cloned().collect();
        assert_eq!(
            observations,
            vec![vmux_layout::worktree::TabDirectoryObserved {
                tab,
                path,
                kind: vmux_layout::worktree::TabDirectoryObservationKind::Read,
            }]
        );
        let previews = app
            .world()
            .resource::<Messages<vmux_layout::OpenBesideRequest>>();
        let mut preview_cursor = previews.get_cursor();
        assert_eq!(
            preview_cursor.read(previews).count(),
            0,
            "file-follow setting still controls preview panes"
        );
    }

    #[test]
    pub(crate) fn file_touch_rejects_command_anchor_mismatched_with_origin() {
        let mut settings = test_settings();
        settings.agent.follow_files = false;
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_layout::LayoutContractPlugin,
            vmux_editor::EditorContractPlugin,
        ))
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_core::PageOpenRequest>()
        .insert_resource(settings)
        .add_systems(Update, handle_agent_file_touch);

        let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
            .id();
        let command_anchor = ProcessId::new();
        app.world_mut().spawn((command_anchor, ChildOf(stack)));
        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(ProcessId::new()),
                },
                command: ServiceAgentCommand::FileTouched {
                    anchor: command_anchor,
                    path: std::env::temp_dir()
                        .join("vmux-mismatched-anchor.rs")
                        .to_string_lossy()
                        .into_owned(),
                    line: None,
                    col: None,
                    end_col: None,
                    kind: vmux_service::protocol::FileTouchKind::Read,
                },
            });

        app.update();

        let messages = app
            .world()
            .resource::<Messages<vmux_layout::worktree::TabDirectoryObserved>>();
        let mut cursor = messages.get_cursor();
        assert_eq!(cursor.read(messages).count(), 0);
    }

    #[test]
    pub(crate) fn edit_file_touch_rebinds_tab_in_same_frame() {
        #[derive(Resource)]
        struct RunTab(Entity);

        #[derive(Resource, Default)]
        struct CapturedRunCwd(Option<PathBuf>);

        fn capture_run_cwd(
            mut reader: MessageReader<AgentCommandRequest>,
            run_tab: Res<RunTab>,
            tabs: Query<&vmux_layout::tab::Tab>,
            mut captured: ResMut<CapturedRunCwd>,
        ) {
            for request in reader.read() {
                if matches!(request.command, ServiceAgentCommand::Run { .. }) {
                    let tab = tabs.get(run_tab.0).unwrap();
                    captured.0 = AgentCwd::of_tab(tab.startup_dir.as_deref())
                        .or_agent_launch(None)
                        .ok();
                }
            }
        }

        struct TestRepo(PathBuf);

        impl TestRepo {
            fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for TestRepo {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        fn git(dir: &Path, args: &[&str]) {
            let status = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .status()
                .unwrap();
            assert!(status.success());
        }

        fn init_repo(name: &str) -> TestRepo {
            let path = std::env::temp_dir().join(format!(
                "vmux-agent-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).unwrap();
            let repo = TestRepo(path);
            git(repo.path(), &["init", "-q", "-b", "main"]);
            git(repo.path(), &["config", "user.email", "t@example.com"]);
            git(repo.path(), &["config", "user.name", "Test"]);
            git(repo.path(), &["config", "commit.gpgsign", "false"]);
            std::fs::write(repo.path().join("seed.txt"), "seed\n").unwrap();
            git(repo.path(), &["add", "seed.txt"]);
            git(repo.path(), &["commit", "-qm", "init"]);
            repo
        }

        let current = init_repo("current");
        let observed = init_repo("observed");
        let expected = observed
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut settings = test_settings();
        settings.agent.follow_files = false;
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_layout::worktree::WorktreePlugin,
            vmux_layout::LayoutContractPlugin,
            vmux_editor::EditorContractPlugin,
        ))
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_core::PageOpenRequest>()
        .init_resource::<CapturedRunCwd>()
        .insert_resource(settings)
        .add_systems(
            Update,
            (
                handle_agent_file_touch.before(vmux_layout::worktree::TabDirectoryRebindSet),
                capture_run_cwd.after(vmux_layout::worktree::TabDirectoryRebindSet),
            ),
        );
        let tab = app
            .world_mut()
            .spawn(vmux_layout::tab::Tab {
                name: "test".into(),
                startup_dir: Some(current.path().to_string_lossy().into_owned()),
            })
            .id();
        app.insert_resource(RunTab(tab));
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
            .id();
        let anchor = ProcessId::new();
        app.world_mut().spawn((anchor, ChildOf(stack)));
        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                command: ServiceAgentCommand::FileTouched {
                    anchor,
                    path: observed
                        .path()
                        .join("seed.txt")
                        .to_string_lossy()
                        .into_owned(),
                    line: None,
                    col: None,
                    end_col: None,
                    kind: vmux_service::protocol::FileTouchKind::Edit,
                },
            });
        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                command: ServiceAgentCommand::Run {
                    anchor,
                    command: "pwd".into(),
                    direction: vmux_service::protocol::AgentPaneDirection::Right,
                    focus: false,
                    beside: None,
                    mode: vmux_service::protocol::PlacementMode::Auto,
                    terminal: None,
                    done_marker: None,
                },
            });

        app.update();

        assert_eq!(
            app.world()
                .get::<vmux_layout::tab::Tab>(tab)
                .unwrap()
                .startup_dir
                .as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(
            app.world().resource::<CapturedRunCwd>().0.as_deref(),
            Some(observed.path().canonicalize().unwrap().as_path())
        );
    }

    #[test]
    pub(crate) fn tidy_page_on_idle_closes_clean_previews_for_native_chat_cli() {
        let mut settings = test_settings();
        settings.agent.tidy_files_auto = true;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .add_message::<vmux_core::PageOpenRequest>()
            .insert_resource(settings)
            .add_systems(Update, tidy_page_on_idle);

        let parent = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
        let agent_pane = app.world_mut().spawn((Pane, ChildOf(parent))).id();
        let agent_stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::stack_bundle(),
                vmux_session::AgentSession {
                    kind: vmux_core::agent::AgentKind::Claude,
                    variant: crate::AgentVariant::Cli,
                    sid: "sid-1".to_string(),
                    provider: "claude".to_string(),
                    model: "cli".to_string(),
                },
                crate::AgentRunState::Streaming,
                ChildOf(agent_pane),
            ))
            .id();
        let file_pane = app.world_mut().spawn((Pane, ChildOf(parent))).id();
        let previews: Vec<Entity> = (0..6)
            .map(|i| {
                spawn_file_preview_stack(&mut app, file_pane, i, &format!("file:///clean/f{i}.rs"))
            })
            .collect();

        app.update();
        assert!(
            close_stack_requests(&app).is_empty(),
            "streaming (not idle) must not tidy"
        );

        *app.world_mut()
            .get_mut::<crate::AgentRunState>(agent_stack)
            .unwrap() = crate::AgentRunState::Idle;
        app.update();

        let mut closed = close_stack_requests(&app);
        closed.sort();
        let mut expected = previews[0..5].to_vec();
        expected.sort();
        assert_eq!(
            closed, expected,
            "clean non-active previews close; the active (max LastActivatedAt) preview is kept"
        );
        assert!(
            !closed.contains(&previews[5]),
            "active preview must be kept"
        );
    }
}
