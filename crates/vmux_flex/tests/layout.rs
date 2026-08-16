//! The geometry this engine is required to produce, frozen while `bevy_ui` was here to vouch for
//! it.
//!
//! Every number in `GOLDEN` and `MUTATION_GOLDEN` was asserted equal to `bevy_ui`'s own output
//! before it was written down — exactly, not within an epsilon, because `bevy_ui` wraps the same
//! `taffy` this crate uses. They came out of a different implementation, which is what makes them
//! an independent oracle rather than a restatement of what this engine happens to do today.
//!
//! Regenerate only by putting the oracle back: re-add the `bevy_ui` feature, restore the
//! differential harness, and run `emit_golden_tables`. A table edited to match new behaviour
//! proves nothing.

use bevy::prelude::*;
use vmux_flex::{
    AlignItems, ComputedNode as FlexComputed, Display, FlexDirection, FlexPlugin, JustifyContent,
    Node as FlexNode, PositionType, UiRect, Val,
};

/// A tree authored in this crate's types, labelled so a mismatch names a path rather than an id.
struct TreeSpec {
    label: String,
    node: FlexNode,
    children: Vec<TreeSpec>,
}

impl TreeSpec {
    fn new(label: impl Into<String>, node: FlexNode) -> Self {
        Self {
            label: label.into(),
            node,
            children: Vec::new(),
        }
    }

    fn with(mut self, child: TreeSpec) -> Self {
        self.children.push(child);
        self
    }

    fn spawn(&self, world: &mut World, parent: Option<Entity>) -> Vec<Labelled> {
        let id = world.spawn(self.node.clone()).id();
        if let Some(parent) = parent {
            world.entity_mut(id).insert(ChildOf(parent));
        }
        let mut out = vec![Labelled {
            label: self.label.clone(),
            entity: id,
        }];
        for child in &self.children {
            out.extend(child.spawn(world, Some(id)));
        }
        out
    }
}

struct Labelled {
    label: String,
    entity: Entity,
}

struct Harness {
    app: App,
    nodes: Vec<Labelled>,
}

impl Harness {
    /// `WindowPlugin` spawns the primary window itself, so the resolution goes in through the
    /// plugin — spawning a second one leaves two `PrimaryWindow`s and every `Single` over them
    /// silently matches nothing.
    fn app(size: UVec2, scale_factor: f32) -> App {
        let mut resolution = bevy::window::WindowResolution::default();
        resolution.set_scale_factor_override(Some(scale_factor));
        resolution.set_physical_resolution(size.x, size.y);

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .add_plugins(bevy::window::WindowPlugin {
                primary_window: Some(Window {
                    resolution,
                    ..default()
                }),
                ..default()
            })
            .add_plugins(FlexPlugin);

        app
    }

    fn start(spec: &TreeSpec, size: UVec2, scale_factor: f32) -> Self {
        let mut app = Self::app(size, scale_factor);
        let nodes = spec.spawn(app.world_mut(), None);
        let mut harness = Self { app, nodes };
        harness.settle();
        harness
    }

    fn resize(&mut self, size: UVec2, scale_factor: f32) -> Step {
        let world = self.app.world_mut();
        let mut windows = world.query_filtered::<&mut Window, With<bevy::window::PrimaryWindow>>();
        let mut window = windows.single_mut(world).expect("primary window");
        window.resolution.set_physical_resolution(size.x, size.y);
        window
            .resolution
            .set_scale_factor_override(Some(scale_factor));

        self.settle();
        self.step(&format!("resized to {}x{} @{scale_factor}", size.x, size.y))
    }

    fn settle(&mut self) {
        self.app.update();
    }

    /// A layout that changes between two quiescent frames resizes a native view every frame.
    fn assert_stable(&mut self, case: &str) {
        let before: Vec<Option<FlexComputed>> = self
            .nodes
            .iter()
            .map(|node| self.app.world().get::<FlexComputed>(node.entity).copied())
            .collect();
        self.app.update();
        for (node, was) in self.nodes.iter().zip(before) {
            let now = self.app.world().get::<FlexComputed>(node.entity).copied();
            assert_eq!(was, now, "{case} / {}: layout is not quiescent", node.label);
        }
    }
}

/// The window sizes and scale factors the frozen tables are required to span.
///
/// 1.25 and 1.5 are the load-bearing ones: at 1.0 and 2.0 a `Val::Px` scaled before layout and one
/// scaled after are both integral, so those factors cannot tell a correct conversion from an
/// inverted one.
const TARGETS: &[(UVec2, f32)] = &[
    (UVec2::new(1280, 800), 1.0),
    (UVec2::new(1200, 800), 2.0),
    (UVec2::new(1001, 667), 1.25),
    (UVec2::new(1440, 900), 1.5),
    (UVec2::new(2560, 1440), 2.0),
];

fn fill() -> FlexNode {
    FlexNode {
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        ..default()
    }
}

fn inset_zero() -> FlexNode {
    FlexNode {
        position_type: PositionType::Absolute,
        left: Val::ZERO,
        right: Val::ZERO,
        top: Val::ZERO,
        bottom: Val::ZERO,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        ..default()
    }
}

fn leaf(grow: f32) -> FlexNode {
    FlexNode {
        flex_grow: grow,
        flex_basis: Val::ZERO,
        min_width: Val::ZERO,
        min_height: Val::ZERO,
        align_items: AlignItems::Stretch,
        justify_content: JustifyContent::Stretch,
        ..default()
    }
}

/// The shell's own frame: window root, side sheets, the header/main column, then a space, a tab, a
/// pane and a stack. This is the shape every other case is a variation on.
fn production_frame() -> TreeSpec {
    TreeSpec::new(
        "window",
        FlexNode {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Relative,
            flex_direction: FlexDirection::Row,
            padding: UiRect {
                left: Val::ZERO,
                top: Val::ZERO,
                right: Val::Px(8.0),
                bottom: Val::Px(8.0),
            },
            column_gap: Val::Px(6.0),
            ..default()
        },
    )
    .with(TreeSpec::new(
        "sheet.left",
        FlexNode {
            width: Val::Px(260.0),
            height: Val::Percent(100.0),
            min_height: Val::ZERO,
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            ..default()
        },
    ))
    .with(
        TreeSpec::new(
            "main.column",
            FlexNode {
                flex_grow: 1.0,
                flex_basis: Val::ZERO,
                min_width: Val::ZERO,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            },
        )
        .with(TreeSpec::new(
            "header",
            FlexNode {
                width: Val::Percent(100.0),
                height: Val::Px(52.0),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .with(
            TreeSpec::new(
                "main",
                FlexNode {
                    flex_grow: 1.0,
                    min_height: Val::ZERO,
                    ..default()
                },
            )
            .with(
                TreeSpec::new("space", inset_zero()).with(
                    TreeSpec::new("tab", inset_zero()).with(
                        TreeSpec::new("pane.root", leaf(1.0)).with(
                            TreeSpec::new("stack", inset_zero())
                                .with(TreeSpec::new("surface", inset_zero())),
                        ),
                    ),
                ),
            ),
        ),
    )
    .with(TreeSpec::new(
        "sheet.right",
        FlexNode {
            position_type: PositionType::Absolute,
            right: Val::ZERO,
            top: Val::ZERO,
            bottom: Val::ZERO,
            width: Val::Px(320.0),
            display: Display::None,
            ..default()
        },
    ))
    .with(TreeSpec::new(
        "layout.cef",
        FlexNode {
            position_type: PositionType::Absolute,
            left: Val::ZERO,
            top: Val::ZERO,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
    ))
}

/// Deterministic pane trees. Fractional flex division against taffy's rounding is where a
/// hand-written case would not think to look.
fn generated_pane_tree(seed: u64) -> TreeSpec {
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
            self.0 >> 33
        }
    }

    fn build(rng: &mut Rng, depth: u32, path: String, row: bool) -> TreeSpec {
        const GROWS: [f32; 4] = [0.5, 1.0, 1.7, 3.0];
        let grow = GROWS[(rng.next() % 4) as usize];
        if depth == 0 {
            return TreeSpec::new(path, leaf(grow));
        }
        let count = 2 + (rng.next() % 3) as usize;
        let mut split = TreeSpec::new(
            path.clone(),
            FlexNode {
                flex_direction: if row {
                    FlexDirection::Row
                } else {
                    FlexDirection::Column
                },
                column_gap: Val::Px(6.0),
                row_gap: Val::Px(6.0),
                ..leaf(grow)
            },
        );
        for index in 0..count {
            split = split.with(build(rng, depth - 1, format!("{path}.{index}"), !row));
        }
        split
    }

    let mut rng = Rng(seed
        .wrapping_mul(2862933555777941757)
        .wrapping_add(3037000493));
    let depth = 1 + (rng.next() % 5) as u32;
    TreeSpec::new("root", fill()).with(build(&mut rng, depth, "p".to_string(), true))
}

/// Shapes the production frame does not contain but the shell can still produce.
fn edge_cases() -> Vec<(&'static str, TreeSpec, UVec2, f32)> {
    vec![
        (
            "one-pixel window",
            TreeSpec::new("root", fill()),
            UVec2::new(1, 1),
            1.0,
        ),
        (
            "display none subtree",
            TreeSpec::new("root", fill()).with(
                TreeSpec::new(
                    "hidden",
                    FlexNode {
                        display: Display::None,
                        ..fill()
                    },
                )
                .with(TreeSpec::new("hidden.child", fill())),
            ),
            UVec2::new(1280, 800),
            2.0,
        ),
        (
            "absolute inset zero inside padding",
            TreeSpec::new(
                "root",
                FlexNode {
                    padding: UiRect::all(Val::Px(11.0)),
                    ..fill()
                },
            )
            .with(TreeSpec::new("stack", inset_zero())),
            UVec2::new(1001, 667),
            1.5,
        ),
        (
            "absolute with width and three insets",
            TreeSpec::new("root", fill()).with(TreeSpec::new(
                "sheet",
                FlexNode {
                    position_type: PositionType::Absolute,
                    right: Val::ZERO,
                    top: Val::ZERO,
                    bottom: Val::ZERO,
                    width: Val::Px(321.0),
                    ..default()
                },
            )),
            UVec2::new(1440, 900),
            1.25,
        ),
        (
            "no grow with auto basis",
            TreeSpec::new(
                "root",
                FlexNode {
                    flex_direction: FlexDirection::Row,
                    ..fill()
                },
            )
            .with(TreeSpec::new(
                "fixed",
                FlexNode {
                    flex_grow: 0.0,
                    width: Val::Px(137.0),
                    ..default()
                },
            ))
            .with(TreeSpec::new("rest", leaf(1.0))),
            UVec2::new(1001, 667),
            1.25,
        ),
        (
            "bare default node",
            TreeSpec::new("root", FlexNode::default()),
            UVec2::new(1280, 800),
            2.0,
        ),
    ]
}

impl Harness {
    /// Apply an edit, let layout settle, and record the result.
    fn mutate(&mut self, case: &str, edit: impl FnOnce(&mut World, &[Labelled])) -> Step {
        let nodes = std::mem::take(&mut self.nodes);
        edit(self.app.world_mut(), &nodes);
        self.nodes = nodes;
        self.settle();
        // An entity the edit despawned is no longer ours to record.
        self.nodes
            .retain(|node| self.app.world().get_entity(node.entity).is_ok());
        self.step(case)
    }

    /// Require the tree to have settled, then record what layout produced.
    fn step(&mut self, case: &str) -> Step {
        self.assert_stable(case);
        Step {
            label: case.to_string(),
            rows: self.rows(),
        }
    }

    fn entity(&self, label: &str) -> Entity {
        self.nodes
            .iter()
            .find(|node| node.label == label)
            .unwrap_or_else(|| panic!("no node labelled {label}"))
            .entity
    }
}

/// One step of [`Harness::mutation_script`]: what the tree looked like after that edit.
struct Step {
    label: String,
    rows: Vec<(String, Vec2, Vec2)>,
}

impl Harness {
    /// Geometry for every node this harness tracks, in spawn order.
    fn rows(&self) -> Vec<(String, Vec2, Vec2)> {
        let mut out = Vec::new();
        for node in &self.nodes {
            let computed = self
                .app
                .world()
                .get::<FlexComputed>(node.entity)
                .expect("computed");
            out.push((node.label.clone(), computed.size, computed.center));
        }
        out
    }

    /// The edit script whose per-step geometry the golden tables freeze.
    ///
    /// The static corpus checks the style conversion; this checks the incremental tree sync —
    /// adding, removing, reparenting and restyling after the tree already exists, which is where a
    /// stale taffy node or a missed re-parent would show. The differential test and the golden one
    /// replay it identically, so the frozen rows keep that coverage once the oracle is gone.
    fn mutation_script() -> Vec<Step> {
        let mut steps = Vec::new();
        let spec = TreeSpec::new(
            "root",
            FlexNode {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                ..fill()
            },
        )
        .with(TreeSpec::new("a", leaf(1.0)))
        .with(TreeSpec::new("b", leaf(1.0)).with(TreeSpec::new("b.child", leaf(1.0))));

        let mut harness = Harness::start(&spec, UVec2::new(1001, 667), 1.25);
        steps.push(harness.step("initial"));

        let a = harness.entity("a");
        let b = harness.entity("b");
        let b_child = harness.entity("b.child");
        let root = harness.entity("root");

        steps.push(harness.mutate("restyle grow", |world, _| {
            world.entity_mut(a).get_mut::<FlexNode>().unwrap().flex_grow = 2.5;
        }));

        steps.push(harness.mutate("hide a subtree", |world, _| {
            world.entity_mut(b).get_mut::<FlexNode>().unwrap().display = Display::None;
        }));

        steps.push(harness.mutate("show it again", |world, _| {
            world.entity_mut(b).get_mut::<FlexNode>().unwrap().display = Display::Flex;
        }));

        steps.push(harness.mutate("reparent a subtree", |world, _| {
            world.entity_mut(b_child).insert(ChildOf(a));
        }));

        steps.push(harness.mutate("add a child", |world, _| {
            world.spawn((leaf(1.0), ChildOf(root)));
        }));

        // Removing and re-adding `Node` in one frame emits a removal for a live entity; deleting
        // its taffy node on that signal would strand the entity with stale geometry.
        steps.push(
            harness.mutate("remove and reinsert Node in one frame", |world, _| {
                world.entity_mut(a).remove::<FlexNode>();
                world.entity_mut(a).insert(leaf(2.5));
            }),
        );

        steps.push(harness.mutate("despawn a subtree", |world, _| {
            world.entity_mut(b).despawn();
        }));

        steps.push(harness.resize(UVec2::new(1440, 900), 1.25));
        // A scale-factor change has to restyle every `Val::Px`, not just re-run layout.
        steps.push(harness.resize(UVec2::new(1440, 900), 2.0));

        steps
    }
}

/// One frozen tree, laid out at one window size and scale factor.
struct Golden {
    case: &'static str,
    size: UVec2,
    scale: f32,
    /// (label, size x, size y, centre x, centre y)
    rows: &'static [(&'static str, f32, f32, f32, f32)],
}

impl Golden {
    fn spec(&self) -> TreeSpec {
        match self.case.split_once('-') {
            Some(("frame", _)) => production_frame(),
            Some(("panes", seed)) => generated_pane_tree(seed.parse().expect("seed")),
            Some(("edge", index)) => {
                let index: usize = index.parse().expect("index");
                edge_cases().swap_remove(index).1
            }
            _ => panic!("unknown golden case {}", self.case),
        }
    }
}

/// One frozen step of [`Harness::mutation_script`].
struct MutationGolden {
    step: &'static str,
    /// (label, size x, size y, centre x, centre y)
    rows: &'static [(&'static str, f32, f32, f32, f32)],
}

const MUTATION_GOLDEN: &[MutationGolden] = &[
    MutationGolden {
        step: "initial",
        rows: &[
            ("root", 1001.0, 667.0, 500.5, 333.5),
            ("a", 497.0, 667.0, 248.5, 333.5),
            ("b", 497.0, 667.0, 752.5, 333.5),
            ("b.child", 497.0, 667.0, 752.5, 333.5),
        ],
    },
    MutationGolden {
        step: "restyle grow",
        rows: &[
            ("root", 1001.0, 667.0, 500.5, 333.5),
            ("a", 710.0, 667.0, 355.0, 333.5),
            ("b", 284.0, 667.0, 859.0, 333.5),
            ("b.child", 284.0, 667.0, 859.0, 333.5),
        ],
    },
    MutationGolden {
        step: "hide a subtree",
        rows: &[
            ("root", 1001.0, 667.0, 500.5, 333.5),
            ("a", 1001.0, 667.0, 500.5, 333.5),
            ("b", 0.0, 0.0, 0.0, 0.0),
            ("b.child", 0.0, 0.0, 0.0, 0.0),
        ],
    },
    MutationGolden {
        step: "show it again",
        rows: &[
            ("root", 1001.0, 667.0, 500.5, 333.5),
            ("a", 710.0, 667.0, 355.0, 333.5),
            ("b", 284.0, 667.0, 859.0, 333.5),
            ("b.child", 284.0, 667.0, 859.0, 333.5),
        ],
    },
    MutationGolden {
        step: "reparent a subtree",
        rows: &[
            ("root", 1001.0, 667.0, 500.5, 333.5),
            ("a", 710.0, 667.0, 355.0, 333.5),
            ("b", 284.0, 667.0, 859.0, 333.5),
            ("b.child", 710.0, 667.0, 355.0, 333.5),
        ],
    },
    MutationGolden {
        step: "add a child",
        rows: &[
            ("root", 1001.0, 667.0, 500.5, 333.5),
            ("a", 548.0, 667.0, 274.0, 333.5),
            ("b", 219.0, 667.0, 664.5, 333.5),
            ("b.child", 548.0, 667.0, 274.0, 333.5),
        ],
    },
    MutationGolden {
        step: "remove and reinsert Node in one frame",
        rows: &[
            ("root", 1001.0, 667.0, 500.5, 333.5),
            ("a", 548.0, 667.0, 274.0, 333.5),
            ("b", 219.0, 667.0, 664.5, 333.5),
            ("b.child", 548.0, 667.0, 274.0, 333.5),
        ],
    },
    MutationGolden {
        step: "despawn a subtree",
        rows: &[
            ("root", 1001.0, 667.0, 500.5, 333.5),
            ("a", 710.0, 667.0, 355.0, 333.5),
            ("b.child", 710.0, 667.0, 355.0, 333.5),
        ],
    },
    MutationGolden {
        step: "resized to 1440x900 @1.25",
        rows: &[
            ("root", 1440.0, 900.0, 720.0, 450.0),
            ("a", 1023.0, 900.0, 511.5, 450.0),
            ("b.child", 1023.0, 900.0, 511.5, 450.0),
        ],
    },
    MutationGolden {
        step: "resized to 1440x900 @2",
        rows: &[
            ("root", 1440.0, 900.0, 720.0, 450.0),
            ("a", 1020.0, 900.0, 510.0, 450.0),
            ("b.child", 1020.0, 900.0, 510.0, 450.0),
        ],
    },
];

#[test]
fn the_mutation_script_matches_the_frozen_tables() {
    let steps = Harness::mutation_script();
    assert_eq!(
        steps.len(),
        MUTATION_GOLDEN.len(),
        "the script changed length; regenerate with emit_golden_tables"
    );
    for (step, golden) in steps.iter().zip(MUTATION_GOLDEN) {
        assert_eq!(step.label, golden.step, "steps drifted out of order");
        assert_eq!(
            step.rows.len(),
            golden.rows.len(),
            "{}: the tree changed shape",
            golden.step
        );
        for (row, frozen) in step.rows.iter().zip(golden.rows) {
            assert_eq!(row.0, frozen.0, "{}: labels drifted", golden.step);
            assert_eq!(
                row.1,
                Vec2::new(frozen.1, frozen.2),
                "{} / {}: size",
                golden.step,
                row.0
            );
            assert_eq!(
                row.2,
                Vec2::new(frozen.3, frozen.4),
                "{} / {}: centre",
                golden.step,
                row.0
            );
        }
    }
}

const GOLDEN: &[Golden] = &[
    Golden {
        case: "frame-0",
        size: UVec2::new(1280, 800),
        scale: 1.0,
        rows: &[
            ("window", 1280.0, 800.0, 640.0, 400.0),
            ("sheet.left", 260.0, 792.0, 130.0, 396.0),
            ("main.column", 1006.0, 792.0, 769.0, 396.0),
            ("header", 1006.0, 52.0, 769.0, 26.0),
            ("main", 1006.0, 734.0, 769.0, 425.0),
            ("space", 1006.0, 734.0, 769.0, 425.0),
            ("tab", 1006.0, 734.0, 769.0, 425.0),
            ("pane.root", 1006.0, 734.0, 769.0, 425.0),
            ("stack", 1006.0, 734.0, 769.0, 425.0),
            ("surface", 1006.0, 734.0, 769.0, 425.0),
            ("sheet.right", 0.0, 0.0, 0.0, 0.0),
            ("layout.cef", 1280.0, 800.0, 640.0, 400.0),
        ],
    },
    Golden {
        case: "frame-1",
        size: UVec2::new(1200, 800),
        scale: 2.0,
        rows: &[
            ("window", 1200.0, 800.0, 600.0, 400.0),
            ("sheet.left", 520.0, 784.0, 260.0, 392.0),
            ("main.column", 652.0, 784.0, 858.0, 392.0),
            ("header", 652.0, 104.0, 858.0, 52.0),
            ("main", 652.0, 668.0, 858.0, 450.0),
            ("space", 652.0, 668.0, 858.0, 450.0),
            ("tab", 652.0, 668.0, 858.0, 450.0),
            ("pane.root", 652.0, 668.0, 858.0, 450.0),
            ("stack", 652.0, 668.0, 858.0, 450.0),
            ("surface", 652.0, 668.0, 858.0, 450.0),
            ("sheet.right", 0.0, 0.0, 0.0, 0.0),
            ("layout.cef", 1200.0, 800.0, 600.0, 400.0),
        ],
    },
    Golden {
        case: "frame-2",
        size: UVec2::new(1001, 667),
        scale: 1.25,
        rows: &[
            ("window", 1001.0, 667.0, 500.5, 333.5),
            ("sheet.left", 325.0, 657.0, 162.5, 328.5),
            ("main.column", 658.0, 657.0, 662.0, 328.5),
            ("header", 658.0, 65.0, 662.0, 32.5),
            ("main", 658.0, 584.0, 662.0, 365.0),
            ("space", 658.0, 584.0, 662.0, 365.0),
            ("tab", 658.0, 584.0, 662.0, 365.0),
            ("pane.root", 658.0, 584.0, 662.0, 365.0),
            ("stack", 658.0, 584.0, 662.0, 365.0),
            ("surface", 658.0, 584.0, 662.0, 365.0),
            ("sheet.right", 0.0, 0.0, 0.0, 0.0),
            ("layout.cef", 1001.0, 667.0, 500.5, 333.5),
        ],
    },
    Golden {
        case: "frame-3",
        size: UVec2::new(1440, 900),
        scale: 1.5,
        rows: &[
            ("window", 1440.0, 900.0, 720.0, 450.0),
            ("sheet.left", 390.0, 888.0, 195.0, 444.0),
            ("main.column", 1029.0, 888.0, 913.5, 444.0),
            ("header", 1029.0, 78.0, 913.5, 39.0),
            ("main", 1029.0, 801.0, 913.5, 487.5),
            ("space", 1029.0, 801.0, 913.5, 487.5),
            ("tab", 1029.0, 801.0, 913.5, 487.5),
            ("pane.root", 1029.0, 801.0, 913.5, 487.5),
            ("stack", 1029.0, 801.0, 913.5, 487.5),
            ("surface", 1029.0, 801.0, 913.5, 487.5),
            ("sheet.right", 0.0, 0.0, 0.0, 0.0),
            ("layout.cef", 1440.0, 900.0, 720.0, 450.0),
        ],
    },
    Golden {
        case: "frame-4",
        size: UVec2::new(2560, 1440),
        scale: 2.0,
        rows: &[
            ("window", 2560.0, 1440.0, 1280.0, 720.0),
            ("sheet.left", 520.0, 1424.0, 260.0, 712.0),
            ("main.column", 2012.0, 1424.0, 1538.0, 712.0),
            ("header", 2012.0, 104.0, 1538.0, 52.0),
            ("main", 2012.0, 1308.0, 1538.0, 770.0),
            ("space", 2012.0, 1308.0, 1538.0, 770.0),
            ("tab", 2012.0, 1308.0, 1538.0, 770.0),
            ("pane.root", 2012.0, 1308.0, 1538.0, 770.0),
            ("stack", 2012.0, 1308.0, 1538.0, 770.0),
            ("surface", 2012.0, 1308.0, 1538.0, 770.0),
            ("sheet.right", 0.0, 0.0, 0.0, 0.0),
            ("layout.cef", 2560.0, 1440.0, 1280.0, 720.0),
        ],
    },
    Golden {
        case: "panes-0",
        size: UVec2::new(1280, 800),
        scale: 1.0,
        rows: &[
            ("root", 1280.0, 800.0, 640.0, 400.0),
            ("p", 1280.0, 800.0, 640.0, 400.0),
            ("p.0", 667.0, 800.0, 333.5, 400.0),
            ("p.1", 379.0, 800.0, 862.5, 400.0),
            ("p.2", 222.0, 800.0, 1169.0, 400.0),
        ],
    },
    Golden {
        case: "panes-1",
        size: UVec2::new(1200, 800),
        scale: 2.0,
        rows: &[
            ("root", 1200.0, 800.0, 600.0, 400.0),
            ("p", 600.0, 800.0, 300.0, 400.0),
            ("p.0", 45.0, 800.0, 22.5, 400.0),
            ("p.0.0", 45.0, 206.0, 22.5, 103.0),
            ("p.0.0.0", 11.0, 206.0, 5.5, 103.0),
            ("p.0.0.1", 7.0, 206.0, 26.5, 103.0),
            ("p.0.0.2", 3.0, 206.0, 43.5, 103.0),
            ("p.0.1", 45.0, 206.0, 22.5, 321.0),
            ("p.0.1.0", 3.0, 206.0, 1.5, 321.0),
            ("p.0.1.1", 2.0, 206.0, 16.0, 321.0),
            ("p.0.1.2", 3.0, 206.0, 30.5, 321.0),
            ("p.0.1.3", 1.0, 206.0, 44.5, 321.0),
            ("p.0.2", 45.0, 364.0, 22.5, 618.0),
            ("p.0.2.0", 5.0, 364.0, 2.5, 618.0),
            ("p.0.2.1", 8.0, 364.0, 21.0, 618.0),
            ("p.0.2.2", 8.0, 364.0, 41.0, 618.0),
            ("p.1", 155.0, 800.0, 134.5, 400.0),
            ("p.1.0", 155.0, 358.0, 134.5, 179.0),
            ("p.1.0.0", 72.0, 358.0, 93.0, 179.0),
            ("p.1.0.1", 71.0, 358.0, 175.5, 179.0),
            ("p.1.1", 155.0, 358.0, 134.5, 549.0),
            ("p.1.1.0", 56.0, 358.0, 85.0, 549.0),
            ("p.1.1.1", 19.0, 358.0, 134.5, 549.0),
            ("p.1.1.2", 56.0, 358.0, 184.0, 549.0),
            ("p.1.2", 155.0, 60.0, 134.5, 770.0),
            ("p.1.2.0", 57.0, 60.0, 85.5, 770.0),
            ("p.1.2.1", 57.0, 60.0, 154.5, 770.0),
            ("p.1.2.2", 17.0, 60.0, 203.5, 770.0),
            ("p.2", 273.0, 800.0, 360.5, 400.0),
            ("p.2.0", 273.0, 136.0, 360.5, 68.0),
            ("p.2.0.0", 57.0, 136.0, 252.5, 68.0),
            ("p.2.0.1", 56.0, 136.0, 320.0, 68.0),
            ("p.2.0.2", 28.0, 136.0, 375.0, 68.0),
            ("p.2.0.3", 96.0, 136.0, 449.0, 68.0),
            ("p.2.1", 273.0, 232.0, 360.5, 264.0),
            ("p.2.1.0", 109.0, 232.0, 278.5, 264.0),
            ("p.2.1.1", 32.0, 232.0, 361.0, 264.0),
            ("p.2.1.2", 32.0, 232.0, 405.0, 264.0),
            ("p.2.1.3", 64.0, 232.0, 465.0, 264.0),
            ("p.2.2", 273.0, 408.0, 360.5, 596.0),
            ("p.2.2.0", 96.0, 408.0, 272.0, 596.0),
            ("p.2.2.1", 57.0, 408.0, 360.5, 596.0),
            ("p.2.2.2", 96.0, 408.0, 449.0, 596.0),
            ("p.3", 91.0, 800.0, 554.5, 400.0),
            ("p.3.0", 91.0, 285.0, 554.5, 142.5),
            ("p.3.0.0", 40.0, 285.0, 529.0, 142.5),
            ("p.3.0.1", 39.0, 285.0, 579.5, 142.5),
            ("p.3.1", 91.0, 503.0, 554.5, 548.5),
            ("p.3.1.0", 10.0, 503.0, 514.0, 548.5),
            ("p.3.1.1", 21.0, 503.0, 541.5, 548.5),
            ("p.3.1.2", 36.0, 503.0, 582.0, 548.5),
        ],
    },
    Golden {
        case: "panes-2",
        size: UVec2::new(1001, 667),
        scale: 1.25,
        rows: &[
            ("root", 1001.0, 667.0, 500.5, 333.5),
            ("p", 1001.0, 667.0, 500.5, 333.5),
            ("p.0", 264.0, 667.0, 132.0, 333.5),
            ("p.0.0", 264.0, 330.0, 132.0, 165.0),
            ("p.0.1", 264.0, 330.0, 132.0, 502.0),
            ("p.1", 132.0, 667.0, 338.0, 333.5),
            ("p.1.0", 132.0, 145.0, 338.0, 72.5),
            ("p.1.1", 132.0, 73.0, 338.0, 188.5),
            ("p.1.2", 132.0, 435.0, 338.0, 449.5),
            ("p.2", 449.0, 667.0, 636.5, 333.5),
            ("p.2.0", 449.0, 239.0, 636.5, 119.5),
            ("p.2.1", 449.0, 421.0, 636.5, 456.5),
            ("p.3", 132.0, 667.0, 935.0, 333.5),
            ("p.3.0", 132.0, 107.0, 935.0, 53.5),
            ("p.3.1", 132.0, 107.0, 935.0, 168.5),
            ("p.3.2", 132.0, 107.0, 935.0, 283.5),
            ("p.3.3", 132.0, 322.0, 935.0, 506.0),
        ],
    },
    Golden {
        case: "panes-3",
        size: UVec2::new(1440, 900),
        scale: 1.5,
        rows: &[
            ("root", 1440.0, 900.0, 720.0, 450.0),
            ("p", 1440.0, 900.0, 720.0, 450.0),
            ("p.0", 1106.0, 900.0, 553.0, 450.0),
            ("p.1", 325.0, 900.0, 1277.5, 450.0),
        ],
    },
    Golden {
        case: "panes-4",
        size: UVec2::new(2560, 1440),
        scale: 2.0,
        rows: &[
            ("root", 2560.0, 1440.0, 1280.0, 720.0),
            ("p", 2560.0, 1440.0, 1280.0, 720.0),
            ("p.0", 1522.0, 1440.0, 761.0, 720.0),
            ("p.0.0", 1522.0, 899.0, 761.0, 449.5),
            ("p.0.0.0", 503.0, 899.0, 251.5, 449.5),
            ("p.0.0.0.0", 503.0, 219.0, 251.5, 109.5),
            ("p.0.0.0.1", 503.0, 129.0, 251.5, 295.5),
            ("p.0.0.0.2", 503.0, 386.0, 251.5, 565.0),
            ("p.0.0.0.3", 503.0, 129.0, 251.5, 834.5),
            ("p.0.0.1", 1007.0, 899.0, 1018.5, 449.5),
            ("p.0.0.1.0", 1007.0, 685.0, 1018.5, 342.5),
            ("p.0.0.1.1", 1007.0, 202.0, 1018.5, 798.0),
            ("p.0.1", 1522.0, 529.0, 761.0, 1175.5),
            ("p.0.1.0", 120.0, 529.0, 60.0, 1175.5),
            ("p.0.1.0.0", 120.0, 158.0, 60.0, 990.0),
            ("p.0.1.0.1", 120.0, 79.0, 60.0, 1120.5),
            ("p.0.1.0.2", 120.0, 268.0, 60.0, 1306.0),
            ("p.0.1.1", 239.0, 529.0, 251.5, 1175.5),
            ("p.0.1.1.0", 239.0, 94.0, 251.5, 958.0),
            ("p.0.1.1.1", 239.0, 93.0, 251.5, 1062.5),
            ("p.0.1.1.2", 239.0, 318.0, 251.5, 1281.0),
            ("p.0.1.2", 719.0, 529.0, 742.5, 1175.5),
            ("p.0.1.2.0", 719.0, 133.0, 742.5, 977.5),
            ("p.0.1.2.1", 719.0, 67.0, 742.5, 1089.5),
            ("p.0.1.2.2", 719.0, 67.0, 742.5, 1168.5),
            ("p.0.1.2.3", 719.0, 226.0, 742.5, 1326.0),
            ("p.0.1.3", 408.0, 529.0, 1318.0, 1175.5),
            ("p.0.1.3.0", 408.0, 72.0, 1318.0, 947.0),
            ("p.0.1.3.1", 408.0, 217.0, 1318.0, 1103.5),
            ("p.0.1.3.2", 408.0, 216.0, 1318.0, 1332.0),
            ("p.1", 507.0, 1440.0, 1787.5, 720.0),
            ("p.1.0", 507.0, 714.0, 1787.5, 357.0),
            ("p.1.0.0", 41.0, 714.0, 1554.5, 357.0),
            ("p.1.0.0.0", 41.0, 351.0, 1554.5, 175.5),
            ("p.1.0.0.1", 41.0, 351.0, 1554.5, 538.5),
            ("p.1.0.1", 41.0, 714.0, 1607.5, 357.0),
            ("p.1.0.1.0", 41.0, 468.0, 1607.5, 234.0),
            ("p.1.0.1.1", 41.0, 234.0, 1607.5, 597.0),
            ("p.1.0.2", 141.0, 714.0, 1711.5, 357.0),
            ("p.1.0.2.0", 141.0, 183.0, 1711.5, 91.5),
            ("p.1.0.2.1", 141.0, 324.0, 1711.5, 357.0),
            ("p.1.0.2.2", 141.0, 183.0, 1711.5, 622.5),
            ("p.1.0.3", 248.0, 714.0, 1917.0, 357.0),
            ("p.1.0.3.0", 248.0, 468.0, 1917.0, 234.0),
            ("p.1.0.3.1", 248.0, 234.0, 1917.0, 597.0),
            ("p.1.1", 507.0, 714.0, 1787.5, 1083.0),
            ("p.1.1.0", 60.0, 714.0, 1564.0, 1083.0),
            ("p.1.1.0.0", 60.0, 602.0, 1564.0, 1027.0),
            ("p.1.1.0.1", 60.0, 100.0, 1564.0, 1390.0),
            ("p.1.1.1", 362.0, 714.0, 1787.0, 1083.0),
            ("p.1.1.1.0", 362.0, 85.0, 1787.0, 768.5),
            ("p.1.1.1.1", 362.0, 85.0, 1787.0, 865.5),
            ("p.1.1.1.2", 362.0, 254.0, 1787.0, 1047.0),
            ("p.1.1.1.3", 362.0, 254.0, 1787.0, 1313.0),
            ("p.1.1.2", 61.0, 714.0, 2011.5, 1083.0),
            ("p.1.1.2.0", 61.0, 128.0, 2011.5, 790.0),
            ("p.1.1.2.1", 61.0, 434.0, 2011.5, 1083.0),
            ("p.1.1.2.2", 61.0, 128.0, 2011.5, 1376.0),
            ("p.2", 507.0, 1440.0, 2306.5, 720.0),
            ("p.2.0", 507.0, 514.0, 2306.5, 257.0),
            ("p.2.0.0", 210.0, 514.0, 2158.0, 257.0),
            ("p.2.0.0.0", 210.0, 251.0, 2158.0, 125.5),
            ("p.2.0.0.1", 210.0, 251.0, 2158.0, 388.5),
            ("p.2.0.1", 62.0, 514.0, 2307.0, 257.0),
            ("p.2.0.1.0", 62.0, 72.0, 2307.0, 36.0),
            ("p.2.0.1.1", 62.0, 430.0, 2307.0, 299.0),
            ("p.2.0.2", 211.0, 514.0, 2455.5, 257.0),
            ("p.2.0.2.0", 211.0, 72.0, 2455.5, 36.0),
            ("p.2.0.2.1", 211.0, 430.0, 2455.5, 299.0),
            ("p.2.1", 507.0, 85.0, 2306.5, 568.5),
            ("p.2.1.0", 47.0, 85.0, 2076.5, 568.5),
            ("p.2.1.0.0", 47.0, 27.0, 2076.5, 539.5),
            ("p.2.1.0.1", 47.0, 46.0, 2076.5, 588.0),
            ("p.2.1.1", 94.0, 85.0, 2159.0, 568.5),
            ("p.2.1.1.0", 94.0, 36.0, 2159.0, 544.0),
            ("p.2.1.1.1", 94.0, 37.0, 2159.0, 593.5),
            ("p.2.1.2", 47.0, 85.0, 2241.5, 568.5),
            ("p.2.1.2.0", 47.0, 13.0, 2241.5, 532.5),
            ("p.2.1.2.1", 47.0, 24.0, 2241.5, 564.0),
            ("p.2.1.2.2", 47.0, 24.0, 2241.5, 600.0),
            ("p.2.1.3", 283.0, 85.0, 2418.5, 568.5),
            ("p.2.1.3.0", 283.0, 12.0, 2418.5, 532.0),
            ("p.2.1.3.1", 283.0, 25.0, 2418.5, 562.5),
            ("p.2.1.3.2", 283.0, 24.0, 2418.5, 599.0),
            ("p.2.2", 507.0, 291.0, 2306.5, 768.5),
            ("p.2.2.0", 211.0, 291.0, 2158.5, 768.5),
            ("p.2.2.0.0", 211.0, 54.0, 2158.5, 650.0),
            ("p.2.2.0.1", 211.0, 53.0, 2158.5, 714.5),
            ("p.2.2.0.2", 211.0, 160.0, 2158.5, 834.0),
            ("p.2.2.1", 70.0, 291.0, 2311.0, 768.5),
            ("p.2.2.1.0", 70.0, 84.0, 2311.0, 665.0),
            ("p.2.2.1.1", 70.0, 41.0, 2311.0, 738.5),
            ("p.2.2.1.2", 70.0, 142.0, 2311.0, 843.0),
            ("p.2.2.2", 120.0, 291.0, 2418.0, 768.5),
            ("p.2.2.2.0", 120.0, 134.0, 2418.0, 690.0),
            ("p.2.2.2.1", 120.0, 67.0, 2418.0, 802.5),
            ("p.2.2.2.2", 120.0, 66.0, 2418.0, 880.0),
            ("p.2.2.3", 70.0, 291.0, 2525.0, 768.5),
            ("p.2.2.3.0", 70.0, 154.0, 2525.0, 700.0),
            ("p.2.2.3.1", 70.0, 26.0, 2525.0, 802.0),
            ("p.2.2.3.2", 70.0, 87.0, 2525.0, 870.5),
            ("p.2.3", 507.0, 514.0, 2306.5, 1183.0),
            ("p.2.3.0", 161.0, 514.0, 2133.5, 1183.0),
            ("p.2.3.0.0", 161.0, 186.0, 2133.5, 1019.0),
            ("p.2.3.0.1", 161.0, 316.0, 2133.5, 1282.0),
            ("p.2.3.1", 161.0, 514.0, 2306.5, 1183.0),
            ("p.2.3.1.0", 161.0, 316.0, 2306.5, 1084.0),
            ("p.2.3.1.1", 161.0, 186.0, 2306.5, 1347.0),
            ("p.2.3.2", 161.0, 514.0, 2479.5, 1183.0),
            ("p.2.3.2.0", 161.0, 191.0, 2479.5, 1021.5),
            ("p.2.3.2.1", 161.0, 108.0, 2479.5, 1183.0),
            ("p.2.3.2.2", 161.0, 191.0, 2479.5, 1344.5),
        ],
    },
    Golden {
        case: "panes-5",
        size: UVec2::new(1280, 800),
        scale: 1.0,
        rows: &[
            ("root", 1280.0, 800.0, 640.0, 400.0),
            ("p", 640.0, 800.0, 320.0, 400.0),
            ("p.0", 132.0, 800.0, 66.0, 400.0),
            ("p.0.0", 132.0, 265.0, 66.0, 132.5),
            ("p.0.0.0", 28.0, 265.0, 14.0, 132.5),
            ("p.0.0.1", 28.0, 265.0, 48.0, 132.5),
            ("p.0.0.2", 50.0, 265.0, 93.0, 132.5),
            ("p.0.0.3", 8.0, 265.0, 128.0, 132.5),
            ("p.0.1", 132.0, 529.0, 66.0, 535.5),
            ("p.0.1.0", 32.0, 529.0, 16.0, 535.5),
            ("p.0.1.1", 56.0, 529.0, 66.0, 535.5),
            ("p.0.1.2", 32.0, 529.0, 116.0, 535.5),
            ("p.1", 225.0, 800.0, 250.5, 400.0),
            ("p.1.0", 225.0, 180.0, 250.5, 90.0),
            ("p.1.0.0", 43.0, 180.0, 159.5, 90.0),
            ("p.1.0.1", 128.0, 180.0, 251.0, 90.0),
            ("p.1.0.2", 42.0, 180.0, 341.0, 90.0),
            ("p.1.1", 225.0, 614.0, 250.5, 493.0),
            ("p.1.1.0", 83.0, 614.0, 179.5, 493.0),
            ("p.1.1.1", 83.0, 614.0, 268.5, 493.0),
            ("p.1.1.2", 47.0, 614.0, 339.5, 493.0),
            ("p.2", 133.0, 800.0, 435.5, 400.0),
            ("p.2.0", 133.0, 130.0, 435.5, 65.0),
            ("p.2.0.0", 63.0, 130.0, 400.5, 65.0),
            ("p.2.0.1", 64.0, 130.0, 470.0, 65.0),
            ("p.2.1", 133.0, 391.0, 435.5, 331.5),
            ("p.2.1.0", 42.0, 391.0, 390.0, 331.5),
            ("p.2.1.1", 85.0, 391.0, 459.5, 331.5),
            ("p.2.2", 133.0, 131.0, 435.5, 598.5),
            ("p.2.2.0", 40.0, 131.0, 389.0, 598.5),
            ("p.2.2.1", 12.0, 131.0, 421.0, 598.5),
            ("p.2.2.2", 23.0, 131.0, 443.5, 598.5),
            ("p.2.2.3", 40.0, 131.0, 482.0, 598.5),
            ("p.2.3", 133.0, 130.0, 435.5, 735.0),
            ("p.2.3.0", 63.0, 130.0, 400.5, 735.0),
            ("p.2.3.1", 20.0, 130.0, 447.0, 735.0),
            ("p.2.3.2", 11.0, 130.0, 469.5, 735.0),
            ("p.2.3.3", 21.0, 130.0, 491.5, 735.0),
            ("p.3", 132.0, 800.0, 574.0, 400.0),
            ("p.3.0", 132.0, 199.0, 574.0, 99.5),
            ("p.3.0.0", 21.0, 199.0, 518.5, 99.5),
            ("p.3.0.1", 63.0, 199.0, 566.5, 99.5),
            ("p.3.0.2", 36.0, 199.0, 622.0, 99.5),
            ("p.3.1", 132.0, 595.0, 574.0, 502.5),
            ("p.3.1.0", 8.0, 595.0, 512.0, 502.5),
            ("p.3.1.1", 49.0, 595.0, 546.5, 502.5),
            ("p.3.1.2", 49.0, 595.0, 601.5, 502.5),
            ("p.3.1.3", 8.0, 595.0, 636.0, 502.5),
        ],
    },
    Golden {
        case: "edge-0",
        size: UVec2::new(1, 1),
        scale: 1.0,
        rows: &[("root", 1.0, 1.0, 0.5, 0.5)],
    },
    Golden {
        case: "edge-1",
        size: UVec2::new(1280, 800),
        scale: 2.0,
        rows: &[
            ("root", 1280.0, 800.0, 640.0, 400.0),
            ("hidden", 0.0, 0.0, 0.0, 0.0),
            ("hidden.child", 0.0, 0.0, 0.0, 0.0),
        ],
    },
    Golden {
        case: "edge-2",
        size: UVec2::new(1001, 667),
        scale: 1.5,
        rows: &[
            ("root", 1001.0, 667.0, 500.5, 333.5),
            ("stack", 1001.0, 667.0, 500.5, 333.5),
        ],
    },
    Golden {
        case: "edge-3",
        size: UVec2::new(1440, 900),
        scale: 1.25,
        rows: &[
            ("root", 1440.0, 900.0, 720.0, 450.0),
            ("sheet", 401.0, 900.0, 1239.5, 450.0),
        ],
    },
    Golden {
        case: "edge-4",
        size: UVec2::new(1001, 667),
        scale: 1.25,
        rows: &[
            ("root", 1001.0, 667.0, 500.5, 333.5),
            ("fixed", 171.0, 667.0, 85.5, 333.5),
            ("rest", 830.0, 667.0, 586.0, 333.5),
        ],
    },
    Golden {
        case: "edge-5",
        size: UVec2::new(1280, 800),
        scale: 2.0,
        rows: &[("root", 0.0, 0.0, 0.0, 0.0)],
    },
];

/// Regenerating with a shortened target list would narrow the frozen coverage without failing
/// anything else, because every table that survived would still match.
#[test]
fn the_frozen_tables_span_every_target() {
    for (size, scale) in TARGETS {
        assert!(
            GOLDEN
                .iter()
                .any(|golden| golden.size == *size && golden.scale == *scale),
            "no frozen tree at {}x{} @{scale}",
            size.x,
            size.y
        );
    }
}

#[test]
fn geometry_matches_the_frozen_tables() {
    for golden in GOLDEN {
        let harness = Harness::start(&golden.spec(), golden.size, golden.scale);
        assert_eq!(
            harness.nodes.len(),
            golden.rows.len(),
            "{}: the tree changed shape; regenerate with emit_golden_tables",
            golden.case
        );
        for (node, row) in harness.nodes.iter().zip(golden.rows) {
            let computed = harness
                .app
                .world()
                .get::<FlexComputed>(node.entity)
                .expect("computed");
            assert_eq!(node.label, row.0, "{}: labels drifted", golden.case);
            assert_eq!(
                computed.size,
                Vec2::new(row.1, row.2),
                "{} / {}: size",
                golden.case,
                node.label
            );
            assert_eq!(
                computed.center,
                Vec2::new(row.3, row.4),
                "{} / {}: centre",
                golden.case,
                node.label
            );
        }
    }
}
