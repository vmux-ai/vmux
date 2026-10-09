use std::path::{Path, PathBuf};

use bevy::prelude::*;
use vmux_api::protocol::{AgentFileSearch, AgentFileTouched, FileTouchKind, ProcessId};
#[cfg(test)]
use vmux_api::protocol::{AgentRequest, FileSearchMatch};
use vmux_command::WriteCommandRequests;
#[cfg(test)]
use vmux_ecs::PageMetadata;
use vmux_ecs::event::{ExplorerSearchFile, ExplorerSearchMatch, FileViewMode};
use vmux_ecs::service::ServiceMessageSet;
use vmux_ecs::{PageOpenRequest, PageOpenTarget};
#[cfg(test)]
use vmux_editor::ContractPlugin as EditorContractPlugin;
use vmux_editor::{FileViewModeRequest, GlobalSearchRequest};
#[cfg(test)]
use vmux_git::GitDiffSource;
#[cfg(test)]
use vmux_layout::LayoutContractPlugin;
use vmux_layout::OpenBesideRequest;
use vmux_layout::active_pane::ActivatePane;
#[cfg(test)]
use vmux_layout::pane::Pane;
#[cfg(test)]
use vmux_layout::stack::Stack;
#[cfg(test)]
use vmux_layout::tab::Tab;
use vmux_layout::worktree::{
    TabDirectoryObservationKind, TabDirectoryObserved, TabDirectoryRebindSet,
};
use vmux_path::FileUrl;
use vmux_setting::AppSettings;

use super::follow_driver::AgentFileLayout;
use crate::host::event::{AgentRequestInput, CommandOrigin};

pub(super) fn add(app: &mut App) {
    app.add_systems(
        Update,
        (file_touch.before(TabDirectoryRebindSet), file_search)
            .chain()
            .in_set(WriteCommandRequests)
            .after(ServiceMessageSet)
            .after(super::command::CommandSet::Commands),
    );
}

#[derive(bevy::ecs::system::SystemParam)]
struct AgentFileResolve<'w, 's> {
    activate: MessageWriter<'w, ActivatePane>,
    page_open: MessageWriter<'w, PageOpenRequest>,
    open_beside: MessageWriter<'w, OpenBesideRequest>,
    observations: MessageWriter<'w, TabDirectoryObserved>,
    layout: AgentFileLayout<'w, 's>,
}

struct PendingFilePreview {
    anchor: ProcessId,
    agent_pane: Entity,
    url: String,
    request_id: [u8; 16],
    user_origin: bool,
    kind: FileTouchKind,
}

fn file_touch(
    mut reader: MessageReader<AgentRequestInput>,
    mut resolve: AgentFileResolve,
    settings: Res<AppSettings>,
    mut file_view_mode: Option<MessageWriter<FileViewModeRequest>>,
) {
    let mut previews: std::collections::HashMap<Entity, Vec<PendingFilePreview>> =
        std::collections::HashMap::new();
    let mut request_diff_mode = false;
    for request in reader.read() {
        let Ok(Some(command)) = request.decode::<AgentFileTouched>() else {
            continue;
        };
        let anchor = &command.anchor;
        let path = &command.path;
        let line = &command.line;
        let col = &command.col;
        let end_col = &command.end_col;
        let kind = &command.kind;
        if let CommandOrigin::Agent {
            anchor: Some(origin_anchor),
            ..
        } = &request.origin
            && origin_anchor != anchor
        {
            continue;
        }
        if *kind == FileTouchKind::Read
            && Path::new(path).file_name().and_then(|name| name.to_str()) == Some("SKILL.md")
        {
            continue;
        }
        let Some(agent_pane) = resolve.layout.agent_pane(*anchor) else {
            continue;
        };
        if let Some(tab) = resolve.layout.ancestor_tab(agent_pane) {
            let kind = match kind {
                FileTouchKind::Read => TabDirectoryObservationKind::Read,
                FileTouchKind::Edit => TabDirectoryObservationKind::Edit,
            };
            resolve.observations.write(TabDirectoryObserved {
                tab,
                path: PathBuf::from(path),
                kind,
            });
        }
        if !settings.agent.follow_files {
            continue;
        }
        request_diff_mode |= *kind == FileTouchKind::Edit;
        previews
            .entry(agent_pane)
            .or_default()
            .push(PendingFilePreview {
                anchor: *anchor,
                agent_pane,
                url: FileUrl::from_path(Path::new(path), *line, *col, *end_col),
                request_id: request.request_id.0,
                user_origin: !request.origin.is_agent(),
                kind: *kind,
            });
    }
    if request_diff_mode && let Some(file_view_mode) = file_view_mode.as_mut() {
        file_view_mode.write(FileViewModeRequest(FileViewMode::Diff));
    }
    for previews in previews.into_values() {
        let all_reads = previews
            .iter()
            .all(|preview| preview.kind == FileTouchKind::Read);
        let deduped = if all_reads {
            previews.into_iter().last().into_iter().collect()
        } else {
            let mut deduped: Vec<PendingFilePreview> = Vec::new();
            for preview in previews {
                if let Some(existing) = deduped
                    .iter_mut()
                    .find(|existing| resolve.layout.reuses(&preview.url, &existing.url))
                {
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
            let existing = resolve.layout.file_page_for(preview.agent_pane);
            let target = (!open_as_tabs)
                .then(|| {
                    resolve
                        .layout
                        .file_page_target(preview.agent_pane, &preview.url)
                })
                .flatten();
            if let Some(target) = target {
                if target.navigate {
                    resolve.page_open.write(PageOpenRequest {
                        target: PageOpenTarget::Stack(target.stack),
                        url: preview.url,
                        request_id: None,
                    });
                }
            } else {
                resolve.open_beside.write(OpenBesideRequest {
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
                resolve.activate.write(ActivatePane {
                    profile: vmux_layout::active_pane::ProfileId::Agent(format!("{anchor:?}")),
                    active: vmux_layout::active_pane::ActiveStack {
                        tab: None,
                        pane: Some(pane),
                        stack: None,
                    },
                });
            }
        }
    }
}

fn file_search(
    mut reader: MessageReader<AgentRequestInput>,
    mut writer: MessageWriter<GlobalSearchRequest>,
) {
    for request in reader.read() {
        let Ok(Some(command)) = request.decode::<AgentFileSearch>() else {
            continue;
        };
        let mut files: Vec<ExplorerSearchFile> = Vec::new();
        for result in &command.matches {
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
        let Some(first) = files.first() else {
            continue;
        };
        writer.write(GlobalSearchRequest {
            target_path: PathBuf::from(&first.path),
            root: command.root.clone(),
            query: command.query.clone(),
            regex: false,
            case_sensitive: false,
            whole_word: false,
            files,
            capped: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::follow_driver::TestRepo;
    use crate::host::test_support::test_settings;
    use vmux_api::protocol::{AgentFileSearch, AgentFileTouched, AgentRequestId, ProcessId};
    use vmux_layout::pane::PaneSplit;
    use vmux_terminal::AgentCwd;
    use vmux_terminal::{AgentRun, PlacementMode};

    #[test]
    fn file_touch_url_builds_goto_fragment() {
        assert_eq!(
            FileUrl::from_path(Path::new("/a/b.rs"), None, None, None,),
            "file:///a/b.rs"
        );
        assert_eq!(
            FileUrl::from_path(Path::new("/a/b.rs"), Some(10), None, None,),
            "file:///a/b.rs#L10"
        );
        assert_eq!(
            FileUrl::from_path(Path::new("/a/b.rs"), Some(10), Some(5), Some(12),),
            "file:///a/b.rs#L10:5-12"
        );
    }

    pub(crate) fn file_touch_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin, EditorContractPlugin))
            .add_message::<AgentRequestInput>()
            .add_message::<PageOpenRequest>()
            .insert_resource(test_settings())
            .add_systems(Update, file_touch);
        app
    }

    pub(crate) fn spawn_file_touch_layout(
        app: &mut App,
        old_url: &str,
        dirty: bool,
    ) -> (ProcessId, Entity) {
        let tab = app.world_mut().spawn(Tab::default()).id();
        let agent_pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let agent_stack = app
            .world_mut()
            .spawn((Stack::bundle(), ChildOf(agent_pane)))
            .id();
        let anchor = ProcessId::new();
        app.world_mut().spawn((anchor, ChildOf(agent_stack)));
        let file_pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let file_stack = app
            .world_mut()
            .spawn((Stack::bundle(), ChildOf(file_pane)))
            .id();
        app.world_mut().spawn((
            PageMetadata {
                url: old_url.to_string(),
                ..default()
            },
            GitDiffSource { dirty, ..default() },
            ChildOf(file_stack),
        ));
        (anchor, file_stack)
    }

    pub(crate) fn send_file_touch(
        app: &mut App,
        anchor: ProcessId,
        path: &str,
        kind: FileTouchKind,
    ) {
        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                request: AgentRequest::encode(&AgentFileTouched {
                    anchor,
                    path: path.to_string(),
                    line: None,
                    col: None,
                    end_col: None,
                    kind,
                })
                .unwrap(),
            });
    }

    pub(crate) fn send_file_read(app: &mut App, anchor: ProcessId, path: &str) {
        send_file_touch(app, anchor, path, FileTouchKind::Read);
    }

    pub(crate) fn send_file_edit(app: &mut App, anchor: ProcessId, path: &str) {
        send_file_touch(app, anchor, path, FileTouchKind::Edit);
    }

    #[test]
    fn file_read_replaces_clean_follow_stack() {
        let mut app = file_touch_test_app();
        let (anchor, file_stack) = spawn_file_touch_layout(&mut app, "file:///repo/old.rs", false);
        send_file_read(&mut app, anchor, "/repo/new.rs");

        app.update();

        let opens: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect();
        assert_eq!(opens.len(), 1);
        assert!(matches!(
            opens[0].target,
            PageOpenTarget::Stack(stack) if stack == file_stack
        ));
        assert_eq!(opens[0].url, "file:///repo/new.rs");
        let beside = app
            .world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .drain()
            .count();
        assert_eq!(beside, 0);
    }

    #[test]
    fn file_read_replaces_clean_follow_stack_across_nested_split() {
        let mut app = file_touch_test_app();
        let tab = app.world_mut().spawn(Tab::default()).id();
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
            .spawn((Stack::bundle(), ChildOf(agent_pane)))
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
            .spawn((Stack::bundle(), ChildOf(file_pane)))
            .id();
        app.world_mut().spawn((
            PageMetadata {
                url: "file:///repo/old.rs".into(),
                ..default()
            },
            GitDiffSource::default(),
            ChildOf(file_stack),
        ));
        send_file_read(&mut app, anchor, "/repo/new.rs");

        app.update();

        let opens: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect();
        assert_eq!(opens.len(), 1);
        assert!(matches!(
            opens[0].target,
            PageOpenTarget::Stack(stack) if stack == file_stack
        ));
        assert_eq!(opens[0].url, "file:///repo/new.rs");
    }

    #[test]
    fn file_search_forwards_results_to_editor() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, EditorContractPlugin))
            .add_message::<AgentRequestInput>()
            .add_systems(Update, file_search);
        let anchor = ProcessId::new();
        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                request: AgentRequest::encode(&AgentFileSearch {
                    anchor,
                    root: "/repo".into(),
                    query: "needle".into(),
                    matches: vec![
                        FileSearchMatch {
                            path: "/repo/src/main.rs".into(),
                            line: 9,
                            col: 4,
                            end_col: 10,
                            preview: "let needle = true;".into(),
                        },
                        FileSearchMatch {
                            path: "/repo/src/lib.rs".into(),
                            line: 2,
                            col: 0,
                            end_col: 6,
                            preview: "needle".into(),
                        },
                        FileSearchMatch {
                            path: "/repo/src/main.rs".into(),
                            line: 21,
                            col: 8,
                            end_col: 14,
                            preview: "    needle();".into(),
                        },
                    ],
                })
                .unwrap(),
            });

        app.update();

        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<GlobalSearchRequest>>()
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
    fn same_frame_file_reads_replace_once_with_last_touch() {
        let mut app = file_touch_test_app();
        let (anchor, file_stack) = spawn_file_touch_layout(&mut app, "file:///repo/old.rs", false);
        send_file_read(&mut app, anchor, "/repo/first.rs");
        send_file_read(&mut app, anchor, "/repo/second.rs");
        send_file_read(&mut app, anchor, "/repo/first.rs");

        app.update();

        let opens: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect();
        assert_eq!(opens.len(), 1);
        assert!(matches!(
            opens[0].target,
            PageOpenTarget::Stack(stack) if stack == file_stack
        ));
        assert_eq!(opens[0].url, "file:///repo/first.rs");
        let view_modes = app
            .world_mut()
            .resource_mut::<Messages<FileViewModeRequest>>()
            .drain()
            .count();
        assert_eq!(view_modes, 0);
    }

    #[test]
    fn same_frame_file_edits_open_each_distinct_file_as_tabs() {
        let mut app = file_touch_test_app();
        let (anchor, _) = spawn_file_touch_layout(&mut app, "file:///repo/old.rs", false);
        send_file_edit(&mut app, anchor, "/repo/first.rs");
        send_file_edit(&mut app, anchor, "/repo/second.rs");
        send_file_edit(&mut app, anchor, "/repo/first.rs");

        app.update();

        let opens = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .count();
        assert_eq!(opens, 0);
        let beside: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .drain()
            .collect();
        assert_eq!(beside.len(), 2);
        assert_eq!(beside[0].url, "file:///repo/first.rs");
        assert_eq!(beside[1].url, "file:///repo/second.rs");
        let view_modes: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<FileViewModeRequest>>()
            .drain()
            .collect();
        assert_eq!(view_modes, vec![FileViewModeRequest(FileViewMode::Diff)]);
    }

    #[test]
    fn file_read_preserves_dirty_follow_stack() {
        let mut app = file_touch_test_app();
        let (anchor, _) = spawn_file_touch_layout(&mut app, "file:///repo/old.rs", true);
        send_file_read(&mut app, anchor, "/repo/new.rs");

        app.update();

        let opens = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .count();
        assert_eq!(opens, 0);
        let beside: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .drain()
            .collect();
        assert_eq!(beside.len(), 1);
        assert_eq!(beside[0].url, "file:///repo/new.rs");
    }

    #[test]
    fn file_read_does_not_reload_matching_dirty_page() {
        let mut app = file_touch_test_app();
        let (anchor, _) = spawn_file_touch_layout(&mut app, "file:///repo/current.rs", true);
        send_file_read(&mut app, anchor, "/repo/current.rs");

        app.update();

        let opens = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .count();
        let beside = app
            .world_mut()
            .resource_mut::<Messages<OpenBesideRequest>>()
            .drain()
            .count();
        assert_eq!((opens, beside), (0, 0));
    }

    #[test]
    fn skill_file_read_does_not_open_follow_pane() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin, EditorContractPlugin))
            .add_message::<AgentRequestInput>()
            .add_message::<PageOpenRequest>()
            .insert_resource(test_settings())
            .add_systems(Update, file_touch);

        let tab = app.world_mut().spawn(Tab::default()).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app.world_mut().spawn((Stack::bundle(), ChildOf(pane))).id();
        let anchor = ProcessId::new();
        app.world_mut().spawn((anchor, ChildOf(stack)));

        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                request: AgentRequest::encode(&AgentFileTouched {
                    anchor,
                    path: "/Users/me/.agents/skills/caveman/SKILL.md".into(),
                    line: None,
                    col: None,
                    end_col: None,
                    kind: FileTouchKind::Read,
                })
                .unwrap(),
            });

        app.update();

        let previews = app.world().resource::<Messages<OpenBesideRequest>>();
        let mut preview_cursor = previews.get_cursor();
        assert_eq!(preview_cursor.read(previews).count(), 0);
        let observations = app.world().resource::<Messages<TabDirectoryObserved>>();
        let mut observation_cursor = observations.get_cursor();
        assert_eq!(observation_cursor.read(observations).count(), 0);
    }

    #[test]
    fn file_touch_emits_tab_directory_observation_when_file_follow_is_disabled() {
        let mut settings = test_settings();
        settings.agent.follow_files = false;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin, EditorContractPlugin))
            .add_message::<AgentRequestInput>()
            .add_message::<PageOpenRequest>()
            .insert_resource(settings)
            .add_systems(Update, file_touch);

        let tab = app.world_mut().spawn(Tab::default()).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app.world_mut().spawn((Stack::bundle(), ChildOf(pane))).id();
        let anchor = ProcessId::new();
        app.world_mut().spawn((anchor, ChildOf(stack)));
        let path = std::env::temp_dir().join("vmux-observed-file.rs");

        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                request: AgentRequest::encode(&AgentFileTouched {
                    anchor,
                    path: path.to_string_lossy().into_owned(),
                    line: None,
                    col: None,
                    end_col: None,
                    kind: FileTouchKind::Read,
                })
                .unwrap(),
            });

        app.update();

        let messages = app.world().resource::<Messages<TabDirectoryObserved>>();
        let mut cursor = messages.get_cursor();
        let observations: Vec<_> = cursor.read(messages).cloned().collect();
        assert_eq!(
            observations,
            vec![TabDirectoryObserved {
                tab,
                path,
                kind: TabDirectoryObservationKind::Read,
            }]
        );
        let previews = app.world().resource::<Messages<OpenBesideRequest>>();
        let mut preview_cursor = previews.get_cursor();
        assert_eq!(
            preview_cursor.read(previews).count(),
            0,
            "file-follow setting still controls preview panes"
        );
    }

    #[test]
    fn file_touch_rejects_command_anchor_mismatched_with_origin() {
        let mut settings = test_settings();
        settings.agent.follow_files = false;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin, EditorContractPlugin))
            .add_message::<AgentRequestInput>()
            .add_message::<PageOpenRequest>()
            .insert_resource(settings)
            .add_systems(Update, file_touch);

        let tab = app.world_mut().spawn(Tab::default()).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app.world_mut().spawn((Stack::bundle(), ChildOf(pane))).id();
        let command_anchor = ProcessId::new();
        app.world_mut().spawn((command_anchor, ChildOf(stack)));
        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(ProcessId::new()),
                },
                request: AgentRequest::encode(&AgentFileTouched {
                    anchor: command_anchor,
                    path: std::env::temp_dir()
                        .join("vmux-mismatched-anchor.rs")
                        .to_string_lossy()
                        .into_owned(),
                    line: None,
                    col: None,
                    end_col: None,
                    kind: FileTouchKind::Read,
                })
                .unwrap(),
            });

        app.update();

        let messages = app.world().resource::<Messages<TabDirectoryObserved>>();
        let mut cursor = messages.get_cursor();
        assert_eq!(cursor.read(messages).count(), 0);
    }

    #[test]
    fn edit_file_touch_rebinds_tab_in_same_frame() {
        #[derive(Resource)]
        struct RunTab(Entity);

        #[derive(Resource, Default)]
        struct CapturedRunCwd(Option<PathBuf>);

        fn capture_run_cwd(
            mut reader: MessageReader<AgentRequestInput>,
            run_tab: Res<RunTab>,
            tabs: Query<&Tab>,
            mut captured: ResMut<CapturedRunCwd>,
        ) {
            for request in reader.read() {
                if matches!(request.decode::<AgentRun>(), Ok(Some(_))) {
                    let tab = tabs.get(run_tab.0).unwrap();
                    captured.0 = AgentCwd::from_tab(tab.startup_dir.as_deref())
                        .or_agent_launch(None)
                        .ok();
                }
            }
        }

        let current = TestRepo::new("current");
        let observed = TestRepo::new("observed");
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
            LayoutContractPlugin,
            EditorContractPlugin,
        ))
        .add_message::<AgentRequestInput>()
        .add_message::<PageOpenRequest>()
        .init_resource::<CapturedRunCwd>()
        .insert_resource(settings)
        .add_systems(
            Update,
            (
                file_touch.before(TabDirectoryRebindSet),
                capture_run_cwd.after(TabDirectoryRebindSet),
            ),
        );
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "test".into(),
                startup_dir: Some(current.path().to_string_lossy().into_owned()),
            })
            .id();
        app.insert_resource(RunTab(tab));
        let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let stack = app.world_mut().spawn((Stack::bundle(), ChildOf(pane))).id();
        let anchor = ProcessId::new();
        app.world_mut().spawn((anchor, ChildOf(stack)));
        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                request: AgentRequest::encode(&AgentFileTouched {
                    anchor,
                    path: observed
                        .path()
                        .join("seed.txt")
                        .to_string_lossy()
                        .into_owned(),
                    line: None,
                    col: None,
                    end_col: None,
                    kind: FileTouchKind::Edit,
                })
                .unwrap(),
            });
        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                request: AgentRequest::encode(&AgentRun {
                    anchor,
                    command: "pwd".into(),
                    direction: vmux_layout::AgentPaneDirection::Right,
                    focus: false,
                    beside: None,
                    mode: PlacementMode::Auto,
                    terminal: None,
                    done_marker: None,
                })
                .unwrap(),
            });

        app.update();

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(
            app.world().resource::<CapturedRunCwd>().0.as_deref(),
            Some(observed.path().canonicalize().unwrap().as_path())
        );
    }
}
