use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_core::page::PageReady;
use vmux_layout::SpaceFilePresent;
use vmux_layout::cef::LayoutCef;
use vmux_layout::space::Space;
use vmux_layout::stack::Stack;

fn stack_in_active_space(
    stack: Entity,
    child_of_q: &Query<&ChildOf>,
    space_active_q: &Query<Has<vmux_core::Active>, With<Space>>,
) -> bool {
    let mut entity = stack;
    loop {
        if let Ok(active) = space_active_q.get(entity) {
            return active;
        }
        match child_of_q.get(entity) {
            Ok(child_of) => entity = child_of.get(),
            Err(_) => return true,
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
    stacks_q: Query<(Entity, Option<&Children>), With<Stack>>,
    ready_q: Query<(), With<PageReady>>,
    child_of_q: Query<&ChildOf>,
    space_active_q: Query<Has<vmux_core::Active>, With<Space>>,
) {
    let layout_ready = !layout_q.is_empty();

    let mut total_pages = 0usize;
    let mut ready_pages = 0usize;
    for (stack, children) in &stacks_q {
        if !stack_in_active_space(stack, &child_of_q, &space_active_q) {
            continue;
        }
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
#[path = "boot_status.test.rs"]
mod tests;
