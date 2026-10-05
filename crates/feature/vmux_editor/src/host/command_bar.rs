use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::command_bar::OpenRequest;
use vmux_ecs::{CommandBarContribution, CommandBarContributionActivated, CommandBarQueryChanged};

use super::editing::ExLineSubmitted;

#[derive(Component)]
struct EditorContribution(String);

pub(super) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_observer(contribute)
            .add_observer(open_path)
            .add_observer(run_ex);
    }
}

fn contribute(
    trigger: On<CommandBarQueryChanged>,
    existing: Query<(Entity, &ChildOf), With<EditorContribution>>,
    mut commands: Commands,
) {
    let request = trigger.event();
    for (entity, parent) in &existing {
        if parent.parent() == request.target {
            commands.entity(entity).despawn();
        }
    }
    let query = request.query.trim();
    if request.start || !vmux_path::NavigationText::new(query).looks_like_path() {
        return;
    }
    let path = query.to_string();
    commands.spawn((
        Name::new("Editor command-bar row"),
        CommandBarContribution {
            row: vmux_api::command_bar::CommandBarResultItem {
                key: "editor".to_string(),
                leading: "\u{2261}".to_string(),
                title: vmux_ui::i18n::translate("command-open-editor"),
                subtitle: path.clone(),
                file_path: path.clone(),
                ..Default::default()
            },
            rank: if query.ends_with('/') { -100 } else { -200 },
            close: true,
            ..Default::default()
        },
        EditorContribution(path),
        ChildOf(request.target),
    ));
}

fn open_path(
    trigger: On<CommandBarContributionActivated>,
    contributions: Query<&EditorContribution>,
    mut commands: Commands,
) {
    let Ok(contribution) = contributions.get(trigger.event().target) else {
        return;
    };
    commands.trigger(UiInput {
        webview: trigger.event().webview,
        payload: OpenRequest {
            value: format!("file://{}", contribution.0),
            open: trigger.event().open,
        },
    });
}

fn run_ex(
    trigger: On<CommandBarContributionActivated>,
    contributions: Query<&CommandBarContribution>,
    focus: vmux_layout::stack::FocusedStack,
    mut submitted: MessageWriter<ExLineSubmitted>,
) {
    let Ok(contribution) = contributions.get(trigger.event().target) else {
        return;
    };
    if !contribution.row.key.starts_with("editor_ex_") || contribution.value.is_empty() {
        return;
    }
    submitted.write(ExLineSubmitted {
        stack: focus.stack,
        line: contribution.value.clone(),
    });
}
