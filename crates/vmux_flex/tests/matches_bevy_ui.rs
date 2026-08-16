//! Proves this engine computes what `bevy_ui` computes, for the styles the shell actually uses.
//!
//! `bevy_ui` is a wrapper over the same `taffy` this crate depends on, so agreement should be
//! exact rather than approximate — every assertion here is `==` on `f32`, and any difference is a
//! bug in the conversion rather than accumulated error.
//!
//! Both engines run over one world: each entity carries a [`vmux_flex::Node`] and the
//! `bevy::ui::Node` converted from it, and each writes its own computed component. The conversion
//! is the one part a reviewer must read, and it is a field-for-field copy on purpose.
//!
//! **This test dies with `bevy_ui`.** Once the workspace drops that feature it cannot compile, so
//! the geometry it produces is frozen into `golden.rs` first — those tables are what keeps the
//! coverage after the oracle is gone.

use bevy::prelude::*;
use bevy::ui::{ComputedNode as BevyComputed, UiGlobalTransform, UiPlugin, UiTargetCamera};
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

    /// The conversion to `bevy_ui`'s equivalent style. Total, because this crate's style surface is
    /// a strict subset — every field maps, and nothing is defaulted away silently.
    fn to_bevy(node: &FlexNode) -> Node {
        fn val(value: Val) -> bevy::ui::Val {
            match value {
                Val::Auto => bevy::ui::Val::Auto,
                Val::Px(px) => bevy::ui::Val::Px(px),
                Val::Percent(percent) => bevy::ui::Val::Percent(percent),
            }
        }
        fn rect(value: UiRect) -> bevy::ui::UiRect {
            bevy::ui::UiRect {
                left: val(value.left),
                right: val(value.right),
                top: val(value.top),
                bottom: val(value.bottom),
            }
        }
        Node {
            display: match node.display {
                Display::Flex => bevy::ui::Display::Flex,
                Display::None => bevy::ui::Display::None,
            },
            position_type: match node.position_type {
                PositionType::Relative => bevy::ui::PositionType::Relative,
                PositionType::Absolute => bevy::ui::PositionType::Absolute,
            },
            left: val(node.left),
            right: val(node.right),
            top: val(node.top),
            bottom: val(node.bottom),
            width: val(node.width),
            height: val(node.height),
            min_width: val(node.min_width),
            min_height: val(node.min_height),
            padding: rect(node.padding),
            flex_direction: match node.flex_direction {
                FlexDirection::Row => bevy::ui::FlexDirection::Row,
                FlexDirection::Column => bevy::ui::FlexDirection::Column,
            },
            flex_grow: node.flex_grow,
            flex_shrink: node.flex_shrink,
            flex_basis: val(node.flex_basis),
            row_gap: val(node.row_gap),
            column_gap: val(node.column_gap),
            align_items: match node.align_items {
                AlignItems::Default => bevy::ui::AlignItems::Default,
                AlignItems::Stretch => bevy::ui::AlignItems::Stretch,
            },
            justify_content: match node.justify_content {
                JustifyContent::Default => bevy::ui::JustifyContent::Default,
                JustifyContent::Stretch => bevy::ui::JustifyContent::Stretch,
            },
            ..default()
        }
    }

    fn spawn(&self, world: &mut World, camera: Entity, parent: Option<Entity>) -> Vec<Labelled> {
        let mut entity = world.spawn((self.node.clone(), Self::to_bevy(&self.node)));
        if parent.is_none() {
            entity.insert(UiTargetCamera(camera));
        }
        let id = entity.id();
        if let Some(parent) = parent {
            world.entity_mut(id).insert(ChildOf(parent));
        }
        let mut out = vec![Labelled {
            label: self.label.clone(),
            entity: id,
        }];
        for child in &self.children {
            out.extend(child.spawn(world, camera, Some(id)));
        }
        out
    }
}

struct Labelled {
    label: String,
    entity: Entity,
}

/// One world running both engines against the same window.
struct Harness {
    app: App,
    camera: Entity,
    nodes: Vec<Labelled>,
}

impl Harness {
    /// `WindowPlugin` spawns the primary window itself, so the resolution goes in through the
    /// plugin — spawning a second one leaves two `PrimaryWindow`s and every `Single` over them
    /// silently matches nothing.
    fn app(size: UVec2, scale_factor: f32) -> (App, Entity) {
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
            .add_plugins(bevy::image::ImagePlugin::default())
            .add_plugins(bevy::text::TextPlugin)
            .add_plugins(bevy::sprite::SpritePlugin)
            .init_asset::<bevy::mesh::Mesh>()
            .add_plugins(bevy::input::InputPlugin)
            .add_plugins(bevy::a11y::AccessibilityPlugin)
            .add_plugins(bevy::picking::DefaultPickingPlugins)
            .add_plugins(UiPlugin)
            .add_plugins(FlexPlugin);

        let camera = app.world_mut().spawn(bevy::camera::Camera2d).id();
        app.world_mut()
            .entity_mut(camera)
            .get_mut::<bevy::camera::Camera>()
            .expect("Camera2d requires Camera")
            .computed
            .target_info = Some(bevy::camera::RenderTargetInfo {
            physical_size: size,
            scale_factor,
        });
        (app, camera)
    }

    fn start(spec: &TreeSpec, size: UVec2, scale_factor: f32) -> Self {
        let (mut app, camera) = Self::app(size, scale_factor);
        let nodes = spec.spawn(app.world_mut(), camera, None);
        let mut harness = Self { app, camera, nodes };
        harness.settle();
        harness
    }

    /// Resize the window *and* the camera's render target.
    ///
    /// In the app `sync_camera_render_target` mirrors one into the other. `bevy_ui` reads only the
    /// camera and this engine reads only the window, so moving one without the other would compare
    /// two different windows and call the disagreement a bug.
    fn resize(&mut self, size: UVec2, scale_factor: f32) {
        let world = self.app.world_mut();
        let mut windows = world.query_filtered::<&mut Window, With<bevy::window::PrimaryWindow>>();
        let mut window = windows.single_mut(world).expect("primary window");
        window.resolution.set_physical_resolution(size.x, size.y);
        window
            .resolution
            .set_scale_factor_override(Some(scale_factor));

        let camera = self.camera;
        self.app
            .world_mut()
            .entity_mut(camera)
            .get_mut::<bevy::camera::Camera>()
            .expect("Camera2d requires Camera")
            .computed
            .target_info = Some(bevy::camera::RenderTargetInfo {
            physical_size: size,
            scale_factor,
        });

        self.settle();
        self.compare(&format!("resized to {}x{} @{scale_factor}", size.x, size.y));
    }

    /// `bevy_ui` needs two passes to propagate its render target before layout resolves; this
    /// engine converges in one. Three is past both.
    fn settle(&mut self) {
        for _ in 0..3 {
            self.app.update();
        }
    }

    /// Every node's geometry, as this engine and as `bevy_ui` see it.
    fn compare(&self, case: &str) {
        for node in &self.nodes {
            let world = self.app.world();
            let Some(flex) = world.get::<FlexComputed>(node.entity) else {
                panic!("{case} / {}: no FlexComputed", node.label);
            };
            let bevy_size = world
                .get::<BevyComputed>(node.entity)
                .unwrap_or_else(|| panic!("{case} / {}: no bevy ComputedNode", node.label))
                .size;
            let bevy_center = world
                .get::<UiGlobalTransform>(node.entity)
                .unwrap_or_else(|| panic!("{case} / {}: no UiGlobalTransform", node.label))
                .transform_point2(Vec2::ZERO);

            assert_eq!(
                flex.size, bevy_size,
                "{case} / {}: size disagrees with bevy_ui",
                node.label
            );
            assert_eq!(
                flex.center, bevy_center,
                "{case} / {}: centre disagrees with bevy_ui",
                node.label
            );
        }
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

/// Window sizes and scale factors worth crossing every tree with.
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

#[test]
fn the_production_frame_matches_bevy_ui() {
    for (size, scale) in TARGETS {
        let case = format!("frame {}x{} @{scale}", size.x, size.y);
        let mut harness = Harness::start(&production_frame(), *size, *scale);
        harness.compare(&case);
        harness.assert_stable(&case);
    }
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

#[test]
fn generated_pane_trees_match_bevy_ui() {
    for seed in 0..50u64 {
        let (size, scale) = TARGETS[(seed as usize) % TARGETS.len()];
        let case = format!("seed {seed} @{scale}");
        let harness = Harness::start(&generated_pane_tree(seed), size, scale);
        harness.compare(&case);
    }
}

#[test]
fn edge_cases_match_bevy_ui() {
    let cases: Vec<(&str, TreeSpec, UVec2, f32)> = vec![
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
    ];

    for (case, spec, size, scale) in cases {
        let mut harness = Harness::start(&spec, size, scale);
        harness.compare(case);
        harness.assert_stable(case);
    }
}

/// A `Node` whose parent has none is orphaned by `bevy_ui` and never laid out. Reachable here,
/// because a subtree can be spawned with its components arriving in one command buffer.
#[test]
fn a_node_under_a_plain_parent_matches_bevy_ui() {
    let (mut app, camera) = Harness::app(UVec2::new(1280, 800), 2.0);

    let root = app
        .world_mut()
        .spawn((fill(), TreeSpec::to_bevy(&fill()), UiTargetCamera(camera)))
        .id();
    let plain = app.world_mut().spawn(ChildOf(root)).id();
    let buried = app
        .world_mut()
        .spawn((leaf(1.0), TreeSpec::to_bevy(&leaf(1.0)), ChildOf(plain)))
        .id();

    for _ in 0..3 {
        app.update();
    }

    let flex = app.world().get::<FlexComputed>(buried).copied();
    let bevy_size = app
        .world()
        .get::<BevyComputed>(buried)
        .map(|node| node.size);
    assert_eq!(
        flex.map(|node| node.size),
        bevy_size,
        "a node buried under a plain entity must be laid out the same way, or not at all"
    );
}

impl Harness {
    /// Apply an edit, let both engines settle, and require they still agree.
    fn mutate(&mut self, case: &str, edit: impl FnOnce(&mut World, &[Labelled])) {
        let nodes = std::mem::take(&mut self.nodes);
        edit(self.app.world_mut(), &nodes);
        self.nodes = nodes;
        self.settle();
        // An entity the edit despawned is no longer ours to compare.
        self.nodes
            .retain(|node| self.app.world().get_entity(node.entity).is_ok());
        self.compare(case);
    }

    fn entity(&self, label: &str) -> Entity {
        self.nodes
            .iter()
            .find(|node| node.label == label)
            .unwrap_or_else(|| panic!("no node labelled {label}"))
            .entity
    }
}

/// The static corpus checks the style conversion. This checks the incremental tree sync — adding,
/// removing, reparenting and restyling after the tree already exists, which is where a stale taffy
/// node or a missed re-parent would show.
#[test]
fn mutation_sequences_match_bevy_ui() {
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
    harness.compare("initial");

    let a = harness.entity("a");
    let b = harness.entity("b");
    let b_child = harness.entity("b.child");
    let root = harness.entity("root");

    harness.mutate("restyle grow", |world, _| {
        world.entity_mut(a).get_mut::<FlexNode>().unwrap().flex_grow = 2.5;
        world.entity_mut(a).get_mut::<Node>().unwrap().flex_grow = 2.5;
    });

    harness.mutate("hide a subtree", |world, _| {
        world.entity_mut(b).get_mut::<FlexNode>().unwrap().display = Display::None;
        world.entity_mut(b).get_mut::<Node>().unwrap().display = bevy::ui::Display::None;
    });

    harness.mutate("show it again", |world, _| {
        world.entity_mut(b).get_mut::<FlexNode>().unwrap().display = Display::Flex;
        world.entity_mut(b).get_mut::<Node>().unwrap().display = bevy::ui::Display::Flex;
    });

    harness.mutate("reparent a subtree", |world, _| {
        world.entity_mut(b_child).insert(ChildOf(a));
    });

    harness.mutate("add a child", |world, _| {
        world.spawn((leaf(1.0), TreeSpec::to_bevy(&leaf(1.0)), ChildOf(root)));
    });

    // Removing and re-adding `Node` in one frame emits a removal for a live entity; deleting its
    // taffy node on that signal would strand the entity with stale geometry.
    harness.mutate("remove and reinsert Node in one frame", |world, _| {
        world.entity_mut(a).remove::<FlexNode>();
        world.entity_mut(a).insert(leaf(2.5));
    });

    harness.mutate("despawn a subtree", |world, _| {
        world.entity_mut(b).despawn();
    });

    harness.resize(UVec2::new(1440, 900), 1.25);
    // A scale-factor change has to restyle every `Val::Px`, not just re-run layout.
    harness.resize(UVec2::new(1440, 900), 2.0);
}
