use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_ecs::host::persistence::WorkspaceRestore;
use vmux_ecs::page::PageReady;
use vmux_layout::cef::LayoutCef;
use vmux_layout::space::Space;
use vmux_layout::stack::Stack;

pub(crate) struct BootStatusPlugin;

impl Plugin for BootStatusPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(Update, update.after(vmux_layout::stack::ComputeFocusSet));
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((Name::new("Boot status"), SplashStatus::default()));
}

#[derive(bevy::ecs::system::SystemParam)]
struct BootLayout<'w, 's> {
    layout: Query<'w, 's, (), (With<LayoutCef>, With<PageReady>)>,
    stacks: Query<'w, 's, (Entity, Option<&'static Children>), With<Stack>>,
    ready: Query<'w, 's, (), With<PageReady>>,
    child_of: Query<'w, 's, &'static ChildOf>,
    spaces: Query<'w, 's, Has<vmux_ecs::Active>, With<Space>>,
}

impl BootLayout<'_, '_> {
    fn ready(&self) -> bool {
        !self.layout.is_empty()
    }

    fn page_counts(&self) -> (usize, usize) {
        let mut total = 0;
        let mut ready = 0;
        for (stack, children) in &self.stacks {
            if !self.stack_is_active(stack) {
                continue;
            }
            if let Some(children) = children.filter(|children| !children.is_empty()) {
                total += 1;
                if children.iter().any(|entity| self.ready.contains(entity)) {
                    ready += 1;
                }
            }
        }
        (total, ready)
    }

    fn stack_is_active(&self, stack: Entity) -> bool {
        let mut entity = stack;
        loop {
            if let Ok(active) = self.spaces.get(entity) {
                return active;
            }
            match self.child_of.get(entity) {
                Ok(child_of) => entity = child_of.get(),
                Err(_) => return true,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootPhase {
    Starting,
    RestoringSpace,
    LoadingInterface,
    LoadingPages { ready: usize, total: usize },
}

impl BootPhase {
    pub fn display(self) -> String {
        match self {
            BootPhase::Starting => "Starting...".to_string(),
            BootPhase::RestoringSpace => "Restoring space...".to_string(),
            BootPhase::LoadingInterface => "Loading interface...".to_string(),
            BootPhase::LoadingPages { ready, total } => {
                format!("Loading page {ready}/{total}...")
            }
        }
    }
}

#[derive(Component)]
pub struct SplashStatus {
    pub phase: BootPhase,
    #[cfg_attr(
        not(all(target_os = "macos", feature = "native-glass")),
        allow(dead_code)
    )]
    pub reveal_ready: bool,
}

impl Default for SplashStatus {
    fn default() -> Self {
        Self {
            phase: BootPhase::Starting,
            reveal_ready: false,
        }
    }
}

pub struct BootInputs {
    pub space_present: bool,
    pub restore_complete: bool,
    pub layout_ready: bool,
    pub total_pages: usize,
    pub ready_pages: usize,
}

impl BootInputs {
    pub fn status(self) -> SplashStatus {
        let phase = if self.layout_ready && self.total_pages > 0 {
            BootPhase::LoadingPages {
                ready: self.ready_pages,
                total: self.total_pages,
            }
        } else if self.layout_ready || self.restore_complete {
            BootPhase::LoadingInterface
        } else if self.space_present {
            BootPhase::RestoringSpace
        } else {
            BootPhase::Starting
        };

        SplashStatus {
            phase,
            reveal_ready: self.layout_ready,
        }
    }
}

fn update(
    mut status: Single<&mut SplashStatus>,
    restore: Single<&WorkspaceRestore>,
    layout: BootLayout,
) {
    let layout_ready = layout.ready();
    let (total_pages, ready_pages) = layout.page_counts();

    let next = BootInputs {
        space_present: restore.store_present,
        restore_complete: restore.complete,
        layout_ready,
        total_pages,
        ready_pages,
    }
    .status();

    if status.phase != next.phase {
        info!("boot: {}", next.phase.display());
    }
    **status = next;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> BootInputs {
        BootInputs {
            space_present: false,
            restore_complete: false,
            layout_ready: false,
            total_pages: 0,
            ready_pages: 0,
        }
    }

    #[test]
    fn starting_when_nothing_ready() {
        let status = inputs().status();
        assert_eq!(status.phase, BootPhase::Starting);
        assert!(!status.reveal_ready);
    }

    #[test]
    fn restoring_space_when_present_and_not_complete() {
        let status = BootInputs {
            space_present: true,
            ..inputs()
        }
        .status();
        assert_eq!(status.phase, BootPhase::RestoringSpace);
    }

    #[test]
    fn loading_interface_after_restore_complete() {
        let status = BootInputs {
            space_present: true,
            restore_complete: true,
            ..inputs()
        }
        .status();
        assert_eq!(status.phase, BootPhase::LoadingInterface);
    }

    #[test]
    fn loading_interface_on_fresh_boot_once_complete() {
        let status = BootInputs {
            restore_complete: true,
            ..inputs()
        }
        .status();
        assert_eq!(status.phase, BootPhase::LoadingInterface);
    }

    #[test]
    fn loading_pages_counts_when_layout_ready() {
        let status = BootInputs {
            layout_ready: true,
            total_pages: 5,
            ready_pages: 2,
            ..inputs()
        }
        .status();
        assert_eq!(status.phase, BootPhase::LoadingPages { ready: 2, total: 5 });
    }

    #[test]
    fn not_revealed_until_layout_ready() {
        let status = BootInputs {
            layout_ready: false,
            ..inputs()
        }
        .status();
        assert!(!status.reveal_ready);
    }

    #[test]
    fn revealed_when_layout_ready() {
        let status = BootInputs {
            layout_ready: true,
            ..inputs()
        }
        .status();
        assert!(status.reveal_ready);
    }

    #[test]
    fn revealed_when_layout_ready_even_while_pages_pending() {
        let status = BootInputs {
            layout_ready: true,
            total_pages: 3,
            ready_pages: 0,
            ..inputs()
        }
        .status();
        assert!(status.reveal_ready);
    }

    #[test]
    fn display_strings() {
        assert_eq!(BootPhase::Starting.display(), "Starting...");
        assert_eq!(BootPhase::RestoringSpace.display(), "Restoring space...");
        assert_eq!(
            BootPhase::LoadingInterface.display(),
            "Loading interface..."
        );
        assert_eq!(
            BootPhase::LoadingPages { ready: 2, total: 5 }.display(),
            "Loading page 2/5..."
        );
    }

    #[test]
    fn system_reports_loading_pages_and_reveals_on_layout_ready() {
        let mut app = App::new();
        let status = app.world_mut().spawn(SplashStatus::default()).id();
        app.world_mut().spawn(WorkspaceRestore {
            store_present: true,
            complete: false,
        });
        app.add_plugins(MinimalPlugins).add_systems(Update, update);

        app.world_mut().spawn((LayoutCef, PageReady {}));
        let stack = app.world_mut().spawn(Stack::default()).id();
        app.world_mut().spawn((PageReady {}, ChildOf(stack)));

        app.update();

        let status = app.world().get::<SplashStatus>(status).unwrap();
        assert_eq!(status.phase, BootPhase::LoadingPages { ready: 1, total: 1 });
        assert!(status.reveal_ready);
    }

    #[test]
    fn system_reports_restoring_space_before_layout_ready() {
        let mut app = App::new();
        let status = app.world_mut().spawn(SplashStatus::default()).id();
        app.world_mut().spawn(WorkspaceRestore {
            store_present: true,
            complete: false,
        });
        app.add_plugins(MinimalPlugins).add_systems(Update, update);

        app.update();

        let status = app.world().get::<SplashStatus>(status).unwrap();
        assert_eq!(status.phase, BootPhase::RestoringSpace);
        assert!(!status.reveal_ready);
    }
}
