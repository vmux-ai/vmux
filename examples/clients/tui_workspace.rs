use std::io::{self, BufRead, IsTerminal};

use bevy_ecs::prelude::*;
use bevy_math::Vec2;
use vmux_app::flex::{FlexDirection, Val};
use vmux_app::prelude::*;

#[derive(Component)]
struct TuiWorkspace {
    layout_root: Entity,
    flex_row: Entity,
    active_pane: Option<Entity>,
}

#[derive(Component)]
struct TuiPane {
    title: String,
    body: String,
    flex_node: Entity,
}

#[derive(Message)]
enum TuiCommand {
    FocusNext,
    FocusPrev,
    Split { title: String },
}

#[derive(Resource, Default)]
struct RenderedFrame(String);

struct TuiWorkspacePlugin;

impl Plugin for TuiWorkspacePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((MinimalPlugins, FlexPlugin, LayoutPlugin))
            .init_resource::<RenderedFrame>()
            .add_message::<TuiCommand>()
            .add_systems(Startup, setup_workspace)
            .add_systems(
                Update,
                (
                    handle_commands.before(PaneSet::Select),
                    track_selection.in_set(PaneSet::AfterSelect),
                ),
            )
            .add_systems(
                PostUpdate,
                render_frame.after(vmux_app::flex::LayoutSystems::Layout),
            );
    }
}

fn setup_workspace(mut commands: Commands) {
    let viewport = commands
        .spawn(LayoutContext {
            physical_size: Vec2::new(72.0, 14.0),
            scale_factor: 1.0,
        })
        .id();
    let flex_row = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                ..Default::default()
            },
            FlexViewport(viewport),
        ))
        .id();
    let layout_root = commands.spawn(Name::new("TUI Workspace")).id();
    let split = commands
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(layout_root),
        ))
        .id();

    let editor_node = commands
        .spawn((TuiPane::flex_node(), ChildOf(flex_row)))
        .id();
    let editor = commands
        .spawn((
            Pane,
            PaneId("editor".into()),
            TuiPane::new(
                "Editor",
                "fn main() {\n  App::new()\n    .add_plugins(MinimalPlugins)\n    .run();\n}",
                editor_node,
            ),
            ChildOf(split),
        ))
        .id();
    let terminal_node = commands
        .spawn((TuiPane::flex_node(), ChildOf(flex_row)))
        .id();
    commands.spawn((
        Pane,
        PaneId("terminal".into()),
        TuiPane::new(
            "Terminal",
            "$ cargo run -p vmux_examples --example client_tui\nready",
            terminal_node,
        ),
        ChildOf(split),
    ));

    commands.spawn(TuiWorkspace {
        layout_root,
        flex_row,
        active_pane: Some(editor),
    });
    commands.write_message(PaneSelectRequest {
        root: layout_root,
        pane: editor,
    });
}

impl TuiPane {
    fn new(title: &str, body: &str, flex_node: Entity) -> Self {
        Self {
            title: title.to_string(),
            body: body.to_string(),
            flex_node,
        }
    }

    fn flex_node() -> Node {
        Node {
            flex_grow: 1.0,
            height: Val::Percent(100.0),
            ..Default::default()
        }
    }
}

fn handle_commands(
    mut incoming: MessageReader<TuiCommand>,
    workspaces: Query<&TuiWorkspace>,
    splits: Query<(Entity, &Children), With<PaneSplit>>,
    panes: Query<(), With<TuiPane>>,
    mut select: MessageWriter<PaneSelectRequest>,
    mut commands: Commands,
) {
    let Ok(workspace) = workspaces.single() else {
        return;
    };
    let Ok((split, children)) = splits.single() else {
        return;
    };
    let ordered: Vec<Entity> = children
        .iter()
        .filter(|entity| panes.contains(*entity))
        .collect();
    if ordered.is_empty() {
        return;
    }
    let current = workspace
        .active_pane
        .and_then(|active| ordered.iter().position(|entity| *entity == active))
        .unwrap_or(0);
    for command in incoming.read() {
        match command {
            TuiCommand::FocusNext => {
                let next = (current + 1) % ordered.len();
                select.write(PaneSelectRequest {
                    root: workspace.layout_root,
                    pane: ordered[next],
                });
            }
            TuiCommand::FocusPrev => {
                let prev = (current + ordered.len() - 1) % ordered.len();
                select.write(PaneSelectRequest {
                    root: workspace.layout_root,
                    pane: ordered[prev],
                });
            }
            TuiCommand::Split { title } => {
                let flex_node = commands
                    .spawn((TuiPane::flex_node(), ChildOf(workspace.flex_row)))
                    .id();
                let created = commands
                    .spawn((
                        Pane,
                        PaneId(title.to_lowercase()),
                        TuiPane::new(title, "live ECS pane", flex_node),
                        ChildOf(split),
                    ))
                    .id();
                select.write(PaneSelectRequest {
                    root: workspace.layout_root,
                    pane: created,
                });
            }
        }
    }
}

fn track_selection(
    mut changes: MessageReader<PaneSelectionChanged>,
    mut workspaces: Query<&mut TuiWorkspace>,
) {
    let Ok(mut workspace) = workspaces.single_mut() else {
        return;
    };
    for change in changes.read() {
        if change.root == workspace.layout_root {
            workspace.active_pane = Some(change.pane);
        }
    }
}

fn render_frame(
    workspaces: Query<&TuiWorkspace>,
    panes: Query<(Entity, &TuiPane)>,
    computed: Query<&ComputedNode>,
    mut frame: ResMut<RenderedFrame>,
) {
    let active_entity = workspaces
        .single()
        .ok()
        .and_then(|workspace| workspace.active_pane);
    let cols = 72usize;
    let rows = 14usize;
    let mut grid = vec![vec![' '; cols]; rows];
    let mut active_title = String::from("none");

    for (entity, pane) in &panes {
        let Ok(node) = computed.get(pane.flex_node) else {
            continue;
        };
        let logical = node.to_logical();
        let x0 = (logical.center.x - logical.size.x / 2.0).round().max(0.0) as usize;
        let y0 = (logical.center.y - logical.size.y / 2.0).round().max(0.0) as usize;
        let w = (logical.size.x.round().max(4.0) as usize).min(cols.saturating_sub(x0));
        let h = (logical.size.y.round().max(3.0) as usize).min(rows.saturating_sub(y0));
        if w < 4 || h < 3 {
            continue;
        }
        let is_selected = active_entity == Some(entity);
        if is_selected {
            active_title = pane.title.clone();
        }
        RenderedFrame::draw_box(
            &mut grid,
            (x0, y0, w, h),
            &pane.title,
            &pane.body,
            is_selected,
        );
    }

    let mut out = String::new();
    for row in grid {
        let line: String = row.into_iter().collect();
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str(&format!(
        "[Vmux TUI] active={active_title} | commands: h (prev), l (next), s (split), q (quit)"
    ));
    frame.0 = out;
}

impl RenderedFrame {
    fn draw_box(
        grid: &mut [Vec<char>],
        (x0, y0, w, h): (usize, usize, usize, usize),
        title: &str,
        body: &str,
        active: bool,
    ) {
        let (tl, tr, bl, br, hz, vt) = if active {
            ('╔', '╗', '╚', '╝', '═', '║')
        } else {
            ('┌', '┐', '└', '┘', '─', '│')
        };
        let x1 = x0 + w - 1;
        let y1 = y0 + h - 1;
        grid[y0][x0] = tl;
        grid[y0][x1] = tr;
        grid[y1][x0] = bl;
        grid[y1][x1] = br;
        grid[y0][(x0 + 1)..x1].fill(hz);
        grid[y1][(x0 + 1)..x1].fill(hz);
        for row in grid.iter_mut().take(y1).skip(y0 + 1) {
            row[x0] = vt;
            row[x1] = vt;
        }
        let header = if active {
            format!(" {title}* ")
        } else {
            format!(" {title} ")
        };
        for (offset, ch) in header.chars().take(w.saturating_sub(4)).enumerate() {
            grid[y0][x0 + 2 + offset] = ch;
        }
        for (line_idx, line) in body.lines().take(h.saturating_sub(2)).enumerate() {
            for (col_idx, ch) in line.chars().take(w.saturating_sub(4)).enumerate() {
                grid[y0 + 1 + line_idx][x0 + 2 + col_idx] = ch;
            }
        }
    }
}

struct TuiRunnerPlugin {
    interactive: bool,
}

impl Plugin for TuiRunnerPlugin {
    fn build(&self, app: &mut App) {
        let interactive = self.interactive;
        app.set_runner(move |mut app: App| {
            app.finish();
            app.cleanup();
            app.update();
            app.update();
            println!("{}", app.world().resource::<RenderedFrame>().0);
            if !interactive {
                return AppExit::Success;
            }
            let mut split_count = 1usize;
            for line in io::stdin().lock().lines().map_while(Result::ok) {
                let command = match line.trim() {
                    "h" => TuiCommand::FocusPrev,
                    "l" => TuiCommand::FocusNext,
                    "s" => {
                        let title = format!("Pane-{split_count}");
                        split_count += 1;
                        TuiCommand::Split { title }
                    }
                    "q" => break,
                    _ => continue,
                };
                app.world_mut().write_message(command);
                app.update();
                app.update();
                println!("{}", app.world().resource::<RenderedFrame>().0);
            }
            AppExit::Success
        });
    }
}

fn main() {
    let interactive =
        std::env::var_os("VMUX_EXAMPLE_HEADLESS").is_none() && io::stdin().is_terminal();
    App::new()
        .add_plugins((TuiWorkspacePlugin, TuiRunnerPlugin { interactive }))
        .run();
}
