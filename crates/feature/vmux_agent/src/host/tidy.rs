#[cfg(test)]
use std::path::PathBuf;

use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::FileUiStateWrite;
use vmux_ecs::ProcessId;
use vmux_ecs::event::{FileTidyRequest, FileTidyState, TidyChoice};
use vmux_ecs::notify::AgentAttention;
use vmux_ecs::team::Agent;
use vmux_layout::CloseStackRequest;
use vmux_layout::stack::ComputeFocusSet;
#[cfg(test)]
use vmux_path::FileUrl;
use vmux_session::{AcpSession, AgentRunState};
use vmux_setting::{AppSettings, SettingsSaveRequest};

use super::tidy_driver::{PendingTidy, TidyFiles};

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TidySet;

pub(super) fn add(app: &mut App) {
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

fn request(
    trigger: On<UiInput<FileTidyRequest>>,
    child_of: Query<&ChildOf>,
    pending: Query<&PendingTidy>,
    settings: Option<ResMut<AppSettings>>,
    mut save: MessageWriter<SettingsSaveRequest>,
    mut close: MessageWriter<CloseStackRequest>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    commands.trigger(FileUiStateWrite::from_event(
        webview,
        &FileTidyState::default(),
    ));
    let Some(mut settings) = settings else {
        return;
    };
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

fn attention(
    mut reader: MessageReader<AgentAttention>,
    settings: Option<Res<AppSettings>>,
    agents: Query<&ProcessId, With<Agent>>,
    mut tidy: TidyFiles,
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
        let Some(agent_pane) = tidy.agent_pane(*process) else {
            continue;
        };
        tidy.run(agent_pane, &settings);
    }
}

fn idle(
    settings: Option<Res<AppSettings>>,
    sessions: Query<(&AcpSession, &AgentRunState), Changed<AgentRunState>>,
    mut tidy: TidyFiles,
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
        let Some(agent_pane) = tidy.agent_pane(session.anchor) else {
            continue;
        };
        tidy.run(agent_pane, &settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_file_urls() {
        assert_eq!(
            FileUrl::parse("file:///a/b.rs#L3:1-4").and_then(|url| url.path()),
            Some(PathBuf::from("/a/b.rs"))
        );
        assert_eq!(
            FileUrl::parse("file:///a/my%20file.rs").and_then(|url| url.path()),
            Some(PathBuf::from("/a/my file.rs"))
        );
        assert!(FileUrl::parse("file:/rel#x").is_none());
        assert!(FileUrl::parse("https://x/y").is_none());
        assert_eq!(FileUrl::parse("file://").and_then(|url| url.path()), None);
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
        assert!(crate::host::tidy_driver::TidyPolicy::closable(&stacks, 5).is_empty());
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
        let mut got = crate::host::tidy_driver::TidyPolicy::closable(&stacks, 5);
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
        assert!(crate::host::tidy_driver::TidyPolicy::closable(&stacks, 5).is_empty());
    }
}
