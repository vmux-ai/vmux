use std::time::{Duration, Instant};

use bevy::prelude::*;
use vmux_core::page::PageReady;
use vmux_layout::SpaceFilePresent;
use vmux_layout::cef::LayoutCef;
use vmux_layout::stack::{FocusedStack, Stack};

/// How long to wait for the active page after the layout page is ready before
/// revealing the window anyway, so a slow/hanging page cannot stall startup.
pub const ACTIVE_PAGE_BUDGET: Duration = Duration::from_secs(8);

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
    pub active_page_ready: bool,
    pub has_active_page: bool,
    pub elapsed_since_layout: Option<Duration>,
}

pub fn compute(i: BootInputs) -> (BootPhase, bool) {
    let budget_expired = i
        .elapsed_since_layout
        .is_some_and(|e| e >= ACTIVE_PAGE_BUDGET);
    let reveal_ready =
        i.layout_ready && (i.active_page_ready || !i.has_active_page || budget_expired);

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
    focused: Res<FocusedStack>,
    mut layout_ready_at: Local<Option<Instant>>,
) {
    let layout_ready = !layout_q.is_empty();
    if layout_ready && layout_ready_at.is_none() {
        *layout_ready_at = Some(Instant::now());
    }
    let elapsed_since_layout = layout_ready_at.map(|t| t.elapsed());

    // (is_content_stack, has_a_ready_page)
    let inspect = |children: Option<&Children>| -> (bool, bool) {
        match children {
            Some(c) if !c.is_empty() => (true, c.iter().any(|e| ready_q.contains(e))),
            _ => (false, false),
        }
    };

    let mut total_pages = 0usize;
    let mut ready_pages = 0usize;
    for children in &stacks_q {
        let (is_content, ready) = inspect(children);
        if is_content {
            total_pages += 1;
            if ready {
                ready_pages += 1;
            }
        }
    }

    let (has_active_page, active_page_ready) = match focused.stack {
        Some(stack) => match stacks_q.get(stack) {
            Ok(children) => inspect(children),
            Err(_) => (false, false),
        },
        None => (false, false),
    };

    let (phase, reveal_ready) = compute(BootInputs {
        space_present: space_present.0,
        restore_complete: restore.0,
        layout_ready,
        total_pages,
        ready_pages,
        active_page_ready,
        has_active_page,
        elapsed_since_layout,
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
            active_page_ready: false,
            has_active_page: false,
            elapsed_since_layout: None,
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
            has_active_page: true,
            active_page_ready: true,
            ..inputs()
        });
        assert!(!reveal);
    }

    #[test]
    fn revealed_when_layout_and_active_page_ready() {
        let (_, reveal) = compute(BootInputs {
            layout_ready: true,
            has_active_page: true,
            active_page_ready: true,
            ..inputs()
        });
        assert!(reveal);
    }

    #[test]
    fn not_revealed_while_active_page_pending() {
        let (_, reveal) = compute(BootInputs {
            layout_ready: true,
            has_active_page: true,
            active_page_ready: false,
            ..inputs()
        });
        assert!(!reveal);
    }

    #[test]
    fn revealed_via_budget_when_active_page_hangs() {
        let (_, reveal) = compute(BootInputs {
            layout_ready: true,
            has_active_page: true,
            active_page_ready: false,
            elapsed_since_layout: Some(Duration::from_secs(8)),
            ..inputs()
        });
        assert!(reveal);
    }

    #[test]
    fn revealed_when_no_content_pages() {
        let (_, reveal) = compute(BootInputs {
            layout_ready: true,
            has_active_page: false,
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
    fn system_reports_loading_pages_and_reveals_on_active_ready() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SplashStatus>()
            .init_resource::<RestoreComplete>()
            .init_resource::<FocusedStack>()
            .insert_resource(SpaceFilePresent(true))
            .add_systems(Update, compute_boot_status);

        app.world_mut().spawn((LayoutCef, PageReady {}));
        let stack = app.world_mut().spawn(Stack::default()).id();
        app.world_mut().spawn((PageReady {}, ChildOf(stack)));
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);

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
            .init_resource::<FocusedStack>()
            .insert_resource(SpaceFilePresent(true))
            .add_systems(Update, compute_boot_status);

        app.update();

        let status = app.world().resource::<SplashStatus>();
        assert_eq!(status.phase, BootPhase::RestoringSpace);
        assert!(!status.reveal_ready);
    }
}
