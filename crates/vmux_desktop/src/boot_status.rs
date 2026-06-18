use bevy::prelude::*;
use vmux_core::page::PageReady;
use vmux_layout::SpaceFilePresent;
use vmux_layout::cef::LayoutCef;
use vmux_layout::stack::Stack;

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

#[derive(Resource)]
pub struct SplashStatus {
    pub phase: BootPhase,
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

/// Set once the saved space has been restored (or immediately when there is no
/// saved space). Owned here, written by the persistence plugin.
#[derive(Resource, Default)]
pub struct RestoreComplete(pub bool);

pub struct BootInputs {
    pub space_present: bool,
    pub restore_complete: bool,
    pub layout_ready: bool,
    pub total_pages: usize,
    pub ready_pages: usize,
}

pub fn compute(i: BootInputs) -> (BootPhase, bool) {
    let reveal_ready = i.layout_ready;

    let phase = if i.layout_ready && i.total_pages > 0 {
        BootPhase::LoadingPages {
            ready: i.ready_pages,
            total: i.total_pages,
        }
    } else if i.layout_ready || i.restore_complete {
        BootPhase::LoadingInterface
    } else if i.space_present {
        BootPhase::RestoringSpace
    } else {
        BootPhase::Starting
    };

    (phase, reveal_ready)
}

pub fn compute_boot_status(
    mut status: ResMut<SplashStatus>,
    space_present: Res<SpaceFilePresent>,
    restore: Res<RestoreComplete>,
    layout_q: Query<(), (With<LayoutCef>, With<PageReady>)>,
    stacks_q: Query<Option<&Children>, With<Stack>>,
    ready_q: Query<(), With<PageReady>>,
) {
    let layout_ready = !layout_q.is_empty();

    let mut total_pages = 0usize;
    let mut ready_pages = 0usize;
    for children in &stacks_q {
        if let Some(c) = children.filter(|c| !c.is_empty()) {
            total_pages += 1;
            if c.iter().any(|e| ready_q.contains(e)) {
                ready_pages += 1;
            }
        }
    }

    let (phase, reveal_ready) = compute(BootInputs {
        space_present: space_present.0,
        restore_complete: restore.0,
        layout_ready,
        total_pages,
        ready_pages,
    });

    if status.phase != phase {
        info!("boot: {}", phase.display());
    }
    status.phase = phase;
    status.reveal_ready = reveal_ready;
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
        let (phase, reveal) = compute(inputs());
        assert_eq!(phase, BootPhase::Starting);
        assert!(!reveal);
    }

    #[test]
    fn restoring_space_when_present_and_not_complete() {
        let (phase, _) = compute(BootInputs {
            space_present: true,
            ..inputs()
        });
        assert_eq!(phase, BootPhase::RestoringSpace);
    }

    #[test]
    fn loading_interface_after_restore_complete() {
        let (phase, _) = compute(BootInputs {
            space_present: true,
            restore_complete: true,
            ..inputs()
        });
        assert_eq!(phase, BootPhase::LoadingInterface);
    }

    #[test]
    fn loading_interface_on_fresh_boot_once_complete() {
        let (phase, _) = compute(BootInputs {
            restore_complete: true,
            ..inputs()
        });
        assert_eq!(phase, BootPhase::LoadingInterface);
    }

    #[test]
    fn loading_pages_counts_when_layout_ready() {
        let (phase, _) = compute(BootInputs {
            layout_ready: true,
            total_pages: 5,
            ready_pages: 2,
            ..inputs()
        });
        assert_eq!(phase, BootPhase::LoadingPages { ready: 2, total: 5 });
    }

    #[test]
    fn not_revealed_until_layout_ready() {
        let (_, reveal) = compute(BootInputs {
            layout_ready: false,
            ..inputs()
        });
        assert!(!reveal);
    }

    #[test]
    fn revealed_when_layout_ready() {
        let (_, reveal) = compute(BootInputs {
            layout_ready: true,
            ..inputs()
        });
        assert!(reveal);
    }

    #[test]
    fn revealed_when_layout_ready_even_while_pages_pending() {
        let (_, reveal) = compute(BootInputs {
            layout_ready: true,
            total_pages: 3,
            ready_pages: 0,
            ..inputs()
        });
        assert!(reveal);
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
        app.add_plugins(MinimalPlugins)
            .init_resource::<SplashStatus>()
            .init_resource::<RestoreComplete>()
            .insert_resource(SpaceFilePresent(true))
            .add_systems(Update, compute_boot_status);

        app.world_mut().spawn((LayoutCef, PageReady {}));
        let stack = app.world_mut().spawn(Stack::default()).id();
        app.world_mut().spawn((PageReady {}, ChildOf(stack)));

        app.update();

        let status = app.world().resource::<SplashStatus>();
        assert_eq!(status.phase, BootPhase::LoadingPages { ready: 1, total: 1 });
        assert!(status.reveal_ready);
    }

    #[test]
    fn system_reports_restoring_space_before_layout_ready() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SplashStatus>()
            .init_resource::<RestoreComplete>()
            .insert_resource(SpaceFilePresent(true))
            .add_systems(Update, compute_boot_status);

        app.update();

        let status = app.world().resource::<SplashStatus>();
        assert_eq!(status.phase, BootPhase::RestoringSpace);
        assert!(!status.reveal_ready);
    }
}
