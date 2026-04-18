use crate::{
    browser::browser_bundle,
    command::{AppCommand, ReadAppCommands, TabCommand},
    layout::pane::Pane,
    layout::space::Space,
    settings::AppSettings,
};
use bevy::prelude::*;
use bevy_cef::prelude::*;

pub(crate) struct TabPlugin;

impl Plugin for TabPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_tab_commands.in_set(ReadAppCommands));
    }
}

#[derive(Component)]
pub(crate) struct Tab;

#[derive(Component)]
pub(crate) struct Active;

pub(crate) fn focused_tab(
    active_space: &Query<Entity, (With<Active>, With<Space>)>,
    _space_children: &Query<&Children, With<Space>>,
    active_pane: &Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: &Query<&Children, With<Pane>>,
    active_tabs: &Query<Entity, (With<Active>, With<Tab>)>,
) -> Option<Entity> {
    let _space = active_space.single().ok()?;
    let pane = active_pane.single().ok()?;
    let children = pane_children.get(pane).ok()?;
    children.iter().find(|&e| active_tabs.contains(e))
}

pub(crate) fn tab_bundle() -> impl Bundle {
    (
        Tab,
        Transform::default(),
        GlobalTransform::default(),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
    )
}

fn handle_tab_commands(
    mut reader: MessageReader<AppCommand>,
    active_space: Query<Entity, (With<Active>, With<Space>)>,
    space_children: Query<&Children, With<Space>>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    tab_q: Query<Entity, With<Tab>>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for cmd in reader.read() {
        let AppCommand::Tab(tab_cmd) = *cmd else {
            continue;
        };

        match tab_cmd {
            TabCommand::New => {
                let Ok(pane) = active_pane.single() else {
                    continue;
                };
                let startup_url = settings.browser.startup_url.as_str();

                if let Ok(children) = pane_children.get(pane) {
                    for child in children.iter() {
                        if tab_q.contains(child) && active_tabs.contains(child) {
                            commands.entity(child).remove::<Active>();
                        }
                    }
                }

                let tab = commands
                    .spawn((tab_bundle(), Active, ChildOf(pane)))
                    .id();
                commands.spawn((
                    browser_bundle(&mut meshes, &mut webview_mt, startup_url),
                    ChildOf(tab),
                ));
            }
            TabCommand::Close => {
                let Ok(pane) = active_pane.single() else {
                    continue;
                };
                let Some(active_tab) = focused_tab(&active_space, &space_children, &active_pane, &pane_children, &active_tabs) else {
                    continue;
                };
                let Ok(children) = pane_children.get(pane) else {
                    continue;
                };
                let tabs_in_pane: Vec<Entity> = children
                    .iter()
                    .filter(|&e| tab_q.contains(e))
                    .collect();
                if tabs_in_pane.len() <= 1 {
                    let startup_url = settings.browser.startup_url.as_str();
                    commands.entity(active_tab).despawn();
                    let tab = commands
                        .spawn((tab_bundle(), Active, ChildOf(pane)))
                        .id();
                    commands.spawn((
                        browser_bundle(&mut meshes, &mut webview_mt, startup_url),
                        ChildOf(tab),
                    ));
                    continue;
                }
                let next = tabs_in_pane
                    .iter()
                    .copied()
                    .find(|&e| e != active_tab)
                    .unwrap();
                commands.entity(active_tab).despawn();
                commands.entity(next).insert(Active);
            }
            TabCommand::Next | TabCommand::Previous => {}
            TabCommand::SelectIndex1
            | TabCommand::SelectIndex2
            | TabCommand::SelectIndex3
            | TabCommand::SelectIndex4
            | TabCommand::SelectIndex5
            | TabCommand::SelectIndex6
            | TabCommand::SelectIndex7
            | TabCommand::SelectIndex8
            | TabCommand::SelectLast => {}
            TabCommand::Reopen
            | TabCommand::Duplicate
            | TabCommand::Pin
            | TabCommand::Mute
            | TabCommand::MoveToPane => {}
        }
    }
}
