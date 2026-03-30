//! Centered command palette (Arc-style): ⌘T / Ctrl+T, glass panel, URL/search row.
//!
//! Register [`VmuxCommandPlugin`] after [`vmux_input::VmuxInputPlugin`]. On [`Startup`], run
//! [`setup`] after the main scene camera exists (e.g. after `vmux`’s `spawn_camera`).

use bevy::camera::ClearColorConfig;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::ui::{BoxShadow, GlobalZIndex, UiTargetCamera};
use bevy_cef::prelude::RequestNavigate;
use leafwing_input_manager::prelude::ActionState;
use vmux_core::{Active, AppInputRoot, VmuxCommandPaletteState};
use vmux_input::AppAction;
use vmux_layout::{Pane, VmuxWebview};

const NUM_ROWS: usize = 3;

const ROW_BG: Color = Color::srgba(0.12, 0.12, 0.14, 0.35);
const ROW_BG_SELECTED: Color = Color::srgb(0.72, 0.42, 0.32);
const ROW_TEXT: Color = Color::srgba(0.92, 0.92, 0.94, 0.95);
const ROW_TEXT_SELECTED: Color = Color::srgba(1.0, 1.0, 1.0, 1.0);
const PANEL_BG: Color = Color::srgba(0.11, 0.11, 0.12, 0.92);
const BORDER_SUBTLE: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);

#[derive(Component)]
struct CommandPaletteUiCamera;

#[derive(Component)]
struct CommandPaletteRoot;

#[derive(Component)]
struct CommandPaletteBackdrop;

#[derive(Component)]
struct CommandPaletteQueryText;

#[derive(Component)]
struct CommandPaletteRow(u8);

fn super_or_ctrl_held(keys: &ButtonInput<KeyCode>) -> bool {
    #[cfg(target_os = "macos")]
    {
        keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight)
    }
    #[cfg(not(target_os = "macos"))]
    {
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
    }
}

fn palette_target_url(query: &str) -> Option<String> {
    let t = query.trim();
    if t.is_empty() {
        return None;
    }
    if t.contains("://") {
        return Some(t.to_string());
    }
    if t.contains('.') && !t.chars().any(char::is_whitespace) {
        return Some(format!("https://{t}"));
    }
    let q: String = t.chars().map(|c| if c == ' ' { '+' } else { c }).collect();
    Some(format!("https://www.google.com/search?q={q}"))
}

fn is_printable_char(chr: char) -> bool {
    let is_in_private_use_area = ('\u{e000}'..='\u{f8ff}').contains(&chr)
        || ('\u{f0000}'..='\u{ffffd}').contains(&chr)
        || ('\u{100000}'..='\u{10fffd}').contains(&chr);
    !is_in_private_use_area && !chr.is_ascii_control()
}

/// Spawns the palette UI camera and root. Run after the main [`vmux_core::VmuxWorldCamera`] exists.
pub fn setup(mut commands: Commands) {
    let camera = commands
        .spawn((
            CommandPaletteUiCamera,
            Camera2d,
            Camera {
                order: 10,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            IsDefaultUiCamera,
        ))
        .id();

    let row_labels_start = [
        "Enter a URL or search terms",
        "Layout — Ctrl+B, then arrows, %, \", …",
        "Close palette (Esc)",
    ];

    commands
        .spawn((
            CommandPaletteRoot,
            UiTargetCamera(camera),
            Node {
                width: percent(100.0),
                height: percent(100.0),
                position_type: PositionType::Relative,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(1),
        ))
        .with_children(|root| {
            root.spawn((
                CommandPaletteBackdrop,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    right: px(0.0),
                    top: px(0.0),
                    bottom: px(0.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.42)),
            ))
            .observe(
                |_: On<Pointer<Press>>, mut palette: ResMut<VmuxCommandPaletteState>| {
                    palette.open = false;
                },
            );

            root.spawn((
                Node {
                    width: percent(90.0),
                    max_width: px(520.0),
                    min_width: px(280.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(14.0)),
                    row_gap: px(6.0),
                    border_radius: BorderRadius::all(px(14.0)),
                    border: UiRect::all(px(1.0)),
                    ..default()
                },
                GlobalZIndex(2),
                BackgroundColor(PANEL_BG),
                BorderColor::all(BORDER_SUBTLE),
                BoxShadow::new(
                    Color::srgba(0.0, 0.0, 0.0, 0.55),
                    px(0.0),
                    px(18.0),
                    px(0.0),
                    px(28.0),
                ),
            ))
            .with_children(|panel| {
                panel
                    .spawn((
                        Node {
                            width: percent(100.0),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: px(10.0),
                            padding: UiRect::axes(px(12.0), px(10.0)),
                            border_radius: BorderRadius::all(px(10.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.25)),
                    ))
                    .with_children(|row| {
                        row.spawn((
                            Text::new("⌕"),
                            TextFont {
                                font_size: 16.0,
                                ..default()
                            },
                            TextColor(ROW_TEXT),
                        ));
                        row.spawn((
                            CommandPaletteQueryText,
                            Text::new("Search or enter URL…"),
                            TextFont {
                                font_size: 15.0,
                                ..default()
                            },
                            TextColor(Color::srgba(0.65, 0.66, 0.70, 1.0)),
                            Node {
                                flex_grow: 1.0,
                                ..default()
                            },
                        ));
                    });

                for (i, label) in row_labels_start.iter().enumerate() {
                    panel
                        .spawn((
                            CommandPaletteRow(i as u8),
                            Node {
                                width: percent(100.0),
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::SpaceBetween,
                                padding: UiRect::axes(px(12.0), px(10.0)),
                                border_radius: BorderRadius::all(px(8.0)),
                                ..default()
                            },
                            BackgroundColor(ROW_BG),
                        ))
                        .with_children(|r| {
                            r.spawn((
                                Text::new(*label),
                                TextFont {
                                    font_size: 14.0,
                                    ..default()
                                },
                                TextColor(ROW_TEXT),
                            ));
                            r.spawn((
                                Text::new("↵"),
                                TextFont {
                                    font_size: 13.0,
                                    ..default()
                                },
                                TextColor(Color::srgba(0.55, 0.56, 0.60, 1.0)),
                            ));
                        });
                }
            });
        });
}

fn sync_visibility(
    palette: Res<VmuxCommandPaletteState>,
    mut q: Query<&mut Visibility, With<CommandPaletteRoot>>,
) {
    if !palette.is_changed() {
        return;
    }
    let Ok(mut vis) = q.single_mut() else {
        return;
    };
    *vis = if palette.open {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}

fn toggle_hotkey(
    state: Query<&ActionState<AppAction>, With<AppInputRoot>>,
    mut palette: ResMut<VmuxCommandPaletteState>,
) {
    let Ok(s) = state.single() else {
        return;
    };
    if s.just_pressed(&AppAction::ToggleCommandPalette) {
        palette.open = !palette.open;
        if palette.open {
            palette.query.clear();
            palette.selection = 0;
        }
    }
}

fn handle_keyboard(
    mut palette: ResMut<VmuxCommandPaletteState>,
    mut reader: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if !palette.open {
        return;
    }

    if keys.just_pressed(KeyCode::Escape) {
        palette.open = false;
        return;
    }

    if keys.just_pressed(KeyCode::ArrowUp) {
        palette.selection = palette.selection.saturating_sub(1);
        return;
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        palette.selection = (palette.selection + 1).min(NUM_ROWS - 1);
        return;
    }

    for ev in reader.read() {
        if !ev.state.is_pressed() {
            continue;
        }

        if ev.key_code == KeyCode::KeyT && super_or_ctrl_held(&keys) {
            continue;
        }

        match (&ev.logical_key, &ev.text) {
            (Key::Backspace, _) => {
                palette.query.pop();
            }
            (_, Some(t)) if !t.is_empty() => {
                for ch in t.chars() {
                    if is_printable_char(ch) {
                        palette.query.push(ch);
                    }
                }
            }
            _ => {}
        }
    }
}

fn submit(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut palette: ResMut<VmuxCommandPaletteState>,
    active: Query<Entity, (With<Pane>, With<Active>, With<VmuxWebview>)>,
) {
    if !palette.open || !keys.just_pressed(KeyCode::Enter) {
        return;
    }
    match palette.selection {
        0 => {
            if let Some(url) = palette_target_url(&palette.query) {
                if let Ok(ent) = active.single() {
                    commands.trigger(RequestNavigate { webview: ent, url });
                }
            }
            palette.open = false;
        }
        2 => {
            palette.open = false;
        }
        _ => {}
    }
}

fn refresh_labels(
    palette: Res<VmuxCommandPaletteState>,
    mut q: Query<&mut Text, With<CommandPaletteQueryText>>,
    mut rows: Query<(&CommandPaletteRow, &Children)>,
    mut text_children: Query<&mut Text, Without<CommandPaletteQueryText>>,
) {
    if !palette.is_changed() && !palette.open {
        return;
    }
    if !palette.open {
        return;
    }

    if let Ok(mut t) = q.single_mut() {
        if palette.query.is_empty() {
            *t = Text::new("Search or enter URL…");
        } else {
            *t = Text::new(palette.query.clone());
        }
    }

    let row0 = if let Some(u) = palette_target_url(&palette.query) {
        let show = u.chars().take(52).collect::<String>();
        let ell = if u.chars().count() > 52 { "…" } else { "" };
        format!("Open {show}{ell}")
    } else {
        "Enter a URL or search terms".to_string()
    };

    let labels = [
        row0.as_str(),
        "Layout — Ctrl+B, then arrows, %, \", …",
        "Close palette (Esc)",
    ];

    for (row, children) in &mut rows {
        let i = row.0 as usize;
        if i >= labels.len() {
            continue;
        }
        let Some(child) = children.iter().next() else {
            continue;
        };
        if let Ok(mut text) = text_children.get_mut(child) {
            *text = Text::new(labels[i]);
        }
    }
}

fn style_rows(
    palette: Res<VmuxCommandPaletteState>,
    mut rows: Query<(&CommandPaletteRow, &mut BackgroundColor, &Children)>,
    mut text_colors: Query<&mut TextColor, Without<CommandPaletteQueryText>>,
) {
    if !palette.is_changed() && !palette.open {
        return;
    }
    if !palette.open {
        return;
    }

    for (row, mut bg, children) in &mut rows {
        let i = row.0 as usize;
        let sel = i == palette.selection;
        *bg = if sel {
            ROW_BG_SELECTED.into()
        } else {
            ROW_BG.into()
        };
        let Some(child) = children.iter().next() else {
            continue;
        };
        if let Ok(mut tc) = text_colors.get_mut(child) {
            *tc = if sel {
                TextColor(ROW_TEXT_SELECTED)
            } else {
                TextColor(ROW_TEXT)
            };
        }
    }
}

/// Command palette resource and [`Update`] systems. Add [`setup`] on [`Startup`] after the world camera.
#[derive(Default)]
pub struct VmuxCommandPlugin;

impl Plugin for VmuxCommandPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VmuxCommandPaletteState>();
        app.add_systems(
            Update,
            (
                toggle_hotkey,
                handle_keyboard,
                submit,
                sync_visibility,
                refresh_labels,
                style_rows,
            )
                .chain(),
        );
    }
}
