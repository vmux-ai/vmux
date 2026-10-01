use std::path::PathBuf;

use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_core::LastActivatedAt;
use vmux_core::ProcessId;
use vmux_core::event::{FileTidyPromptEvent, FileTidyRequest, TidyChoice};
use vmux_core::host::FileUiStateWrite;
use vmux_core::notify::AgentAttention;
use vmux_core::team::Agent;
use vmux_git::GitRepository;
use vmux_layout::CloseStackRequest;
use vmux_layout::stack::ComputeFocusSet;
use vmux_session::{AcpSession, AgentRunState};
use vmux_setting::{AppSettings, SettingsSaveRequest};

use crate::host::follow::AgentFileLayout;

pub struct Plugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TidySet;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentAttention>()
            .add_message::<CloseStackRequest>()
            .add_message::<SettingsSaveRequest>()
            .add_plugins(UiEventPlugin::<(FileTidyRequest,)>::default())
            .add_observer(request)
            .add_systems(
                Update,
                attention
                    .in_set(TidySet)
                    .after(ComputeFocusSet)
                    .after(crate::host::attention::TurnEndedSet),
            )
            .add_systems(Update, idle.after(ComputeFocusSet));
    }
}

#[derive(Component)]
struct PendingTidy {
    closable: Vec<Entity>,
}

fn request(
    trigger: On<UiInput<FileTidyRequest>>,
    child_of: Query<&ChildOf>,
    pending: Query<&PendingTidy>,
    settings: Option<ResMut<AppSettings>>,
    mut save: MessageWriter<SettingsSaveRequest>,
    mut close: MessageWriter<CloseStackRequest>,
    mut commands: Commands,
) {
    let Some(mut settings) = settings else {
        return;
    };
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
    commands.entity(pane).remove::<PendingTidy>();
    match trigger.event().payload.choice {
        TidyChoice::Dismiss => {}
        TidyChoice::Always => {
            settings.agent.tidy_files_auto = true;
            save.write(SettingsSaveRequest);
            for stack in closable {
                close.write(CloseStackRequest::tidying(stack));
            }
        }
        TidyChoice::Tidy => {
            for stack in closable {
                close.write(CloseStackRequest::tidying(stack));
            }
        }
    }
}

fn path_from_file_url(url: &str) -> Option<PathBuf> {
    let rest = url
        .strip_prefix("file://")
        .or_else(|| url.strip_prefix("file:"))?;
    let no_frag = rest.split('#').next().unwrap_or(rest);
    let decoded = percent_decode(no_frag);
    if decoded.is_empty() {
        return None;
    }
    Some(PathBuf::from(decoded))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decide_closable(stacks: &[(Entity, i64, bool)], max: usize) -> Vec<Entity> {
    if stacks.len() <= max {
        return Vec::new();
    }
    let active = stacks
        .iter()
        .max_by_key(|(_, ts, _)| *ts)
        .map(|(s, _, _)| *s);
    stacks
        .iter()
        .filter(|(s, _, changed)| Some(*s) != active && !changed)
        .map(|(s, _, _)| *s)
        .collect()
}

fn is_changed(
    abs: &std::path::Path,
    repos: &mut Vec<(PathBuf, std::collections::HashSet<String>)>,
) -> bool {
    let abs = abs.canonicalize().unwrap_or_else(|_| abs.to_path_buf());
    if let Some((root, set)) = repos.iter().find(|(r, _)| abs.starts_with(r)) {
        return set.contains(&rel_str(root, &abs));
    }
    match GitRepository::discover(&abs)
        .and_then(|repository| repository.dirty_paths().map(|set| (repository, set)))
    {
        Ok((repository, set)) => {
            let root = repository.path().to_path_buf();
            let changed = set.contains(&rel_str(&root, &abs));
            repos.push((root, set));
            changed
        }
        Err(_) => false,
    }
}

fn rel_str(root: &std::path::Path, abs: &std::path::Path) -> String {
    abs.strip_prefix(root)
        .map(|r| r.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn tidy_follow_pane(
    agent_pane: Entity,
    settings: &AppSettings,
    layout: &AgentFileLayout,
    last_activated: &Query<&LastActivatedAt>,
    pending: &Query<(), With<PendingTidy>>,
    close: &mut MessageWriter<CloseStackRequest>,
    commands: &mut Commands,
) {
    let Some((follow_pane, stacks)) = layout.file_stacks_for(agent_pane) else {
        return;
    };
    if pending.get(follow_pane).is_ok() {
        return;
    }
    let mut repos: Vec<(PathBuf, std::collections::HashSet<String>)> = Vec::new();
    let rows: Vec<(Entity, i64, bool)> = stacks
        .iter()
        .map(|(stack, _page, url)| {
            let timestamp = last_activated
                .get(*stack)
                .map(|timestamp| timestamp.0)
                .unwrap_or(i64::MIN);
            let changed = path_from_file_url(url)
                .map(|path| is_changed(&path, &mut repos))
                .unwrap_or(false);
            (*stack, timestamp, changed)
        })
        .collect();
    let closable = decide_closable(&rows, settings.agent.tidy_files_max);
    if closable.is_empty() {
        return;
    }
    if settings.agent.tidy_files_auto {
        for stack in closable {
            close.write(CloseStackRequest::tidying(stack));
        }
        return;
    }
    let count = closable.len() as u32;
    let active_page = stacks
        .iter()
        .max_by_key(|(stack, _, _)| {
            last_activated
                .get(*stack)
                .map(|timestamp| timestamp.0)
                .unwrap_or(i64::MIN)
        })
        .map(|(_, page, _)| *page);
    if let Some(page) = active_page {
        commands.trigger(FileUiStateWrite::from_event(
            page,
            &FileTidyPromptEvent { count },
        ));
        commands
            .entity(follow_pane)
            .insert(PendingTidy { closable });
    }
}

fn attention(
    mut reader: MessageReader<AgentAttention>,
    settings: Option<Res<AppSettings>>,
    agents: Query<&ProcessId, With<Agent>>,
    layout: AgentFileLayout,
    last_activated: Query<&LastActivatedAt>,
    pending: Query<(), With<PendingTidy>>,
    mut close: MessageWriter<CloseStackRequest>,
    mut commands: Commands,
) {
    let Some(settings) = settings else {
        for _ in reader.read() {}
        return;
    };
    if !settings.agent.tidy_files {
        for _ in reader.read() {}
        return;
    }
    for attention in reader.read() {
        let Ok(process) = agents.get(attention.entity) else {
            continue;
        };
        let Some(agent_pane) = layout.agent_pane(*process) else {
            continue;
        };
        tidy_follow_pane(
            agent_pane,
            &settings,
            &layout,
            &last_activated,
            &pending,
            &mut close,
            &mut commands,
        );
    }
}

fn idle(
    settings: Option<Res<AppSettings>>,
    sessions: Query<(&AcpSession, &AgentRunState), Changed<AgentRunState>>,
    layout: AgentFileLayout,
    last_activated: Query<&LastActivatedAt>,
    pending: Query<(), With<PendingTidy>>,
    mut close: MessageWriter<CloseStackRequest>,
    mut commands: Commands,
) {
    let Some(settings) = settings else {
        return;
    };
    if !settings.agent.tidy_files {
        return;
    }
    for (session, state) in &sessions {
        if !matches!(state, AgentRunState::Idle) {
            continue;
        }
        let Some(agent_pane) = layout.agent_pane(session.anchor) else {
            continue;
        };
        tidy_follow_pane(
            agent_pane,
            &settings,
            &layout,
            &last_activated,
            &pending,
            &mut close,
            &mut commands,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_file_url_stripping_scheme_fragment_and_encoding() {
        assert_eq!(
            path_from_file_url("file:///a/b.rs#L3:1-4"),
            Some(PathBuf::from("/a/b.rs"))
        );
        assert_eq!(
            path_from_file_url("file:///a/my%20file.rs"),
            Some(PathBuf::from("/a/my file.rs"))
        );
        assert_eq!(
            path_from_file_url("file:/rel#x"),
            Some(PathBuf::from("/rel"))
        );
        assert_eq!(path_from_file_url("https://x/y"), None);
        assert_eq!(path_from_file_url("file://"), None);
    }

    #[test]
    fn decide_closable_below_threshold_is_empty() {
        let mut w = World::new();
        let ids: Vec<Entity> = (0..3).map(|_| w.spawn_empty().id()).collect();
        let stacks = vec![
            (ids[0], 10, false),
            (ids[1], 20, false),
            (ids[2], 30, false),
        ];
        assert!(decide_closable(&stacks, 5).is_empty());
    }

    #[test]
    fn decide_closable_keeps_changed_and_active() {
        let mut w = World::new();
        let ids: Vec<Entity> = (0..6).map(|_| w.spawn_empty().id()).collect();
        let stacks = vec![
            (ids[0], 10, false),
            (ids[1], 20, true),
            (ids[2], 30, false),
            (ids[3], 40, true),
            (ids[4], 50, false),
            (ids[5], 60, false),
        ];
        let mut got = decide_closable(&stacks, 5);
        got.sort();
        let mut want = vec![ids[0], ids[2], ids[4]];
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn decide_closable_empty_when_all_changed() {
        let mut w = World::new();
        let ids: Vec<Entity> = (0..6).map(|_| w.spawn_empty().id()).collect();
        let stacks: Vec<(Entity, i64, bool)> = ids
            .iter()
            .enumerate()
            .map(|(i, &e)| (e, i as i64, true))
            .collect();
        assert!(decide_closable(&stacks, 5).is_empty());
    }
}
