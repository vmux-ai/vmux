use crate::{
    command::{AppCommand, ReadAppCommands, SideSheetCommand},
    layout::window::Main,
    settings::AppSettings,
};
use bevy::{prelude::*, ui::UiSystems};
use vmux_header::Header;

pub(crate) struct SideSheetPlugin;

impl Plugin for SideSheetPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SideSheetOpen(true))
            .add_systems(Update, handle_side_sheet_toggle.in_set(ReadAppCommands))
            .add_systems(
                PostUpdate,
                sync_side_sheet_visibility.before(UiSystems::Layout),
            );
    }
}

#[derive(Component)]
pub(crate) struct SideSheet;

#[derive(Resource)]
pub(crate) struct SideSheetOpen(pub bool);

fn handle_side_sheet_toggle(
    mut reader: MessageReader<AppCommand>,
    mut open: ResMut<SideSheetOpen>,
) {
    for cmd in reader.read() {
        if matches!(cmd, AppCommand::SideSheet(SideSheetCommand::Toggle)) {
            open.0 = !open.0;
        }
    }
}

fn sync_side_sheet_visibility(
    open: Res<SideSheetOpen>,
    settings: Res<AppSettings>,
    mut side_sheet_q: Query<(&mut Visibility, &mut Node), With<SideSheet>>,
    mut header_q: Query<&mut Node, (With<Header>, Without<SideSheet>, Without<Main>)>,
    mut main_q: Query<&mut Node, (With<Main>, Without<SideSheet>, Without<Header>)>,
) {
    if !open.is_changed() {
        return;
    }
    let sheet_total = settings.layout.side_sheet.width + settings.layout.pane.gap;
    for (mut vis, mut node) in &mut side_sheet_q {
        if open.0 {
            *vis = Visibility::Inherited;
            node.display = Display::Flex;
        } else {
            *vis = Visibility::Hidden;
            node.display = Display::None;
        }
    }
    for mut node in &mut header_q {
        node.margin.left = if open.0 {
            Val::Px(sheet_total)
        } else {
            Val::Px(0.0)
        };
    }
    for mut node in &mut main_q {
        node.margin.left = if open.0 {
            Val::Px(sheet_total)
        } else {
            Val::Px(0.0)
        };
    }
}
