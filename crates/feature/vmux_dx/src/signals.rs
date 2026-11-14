use std::sync::atomic::{AtomicI32, Ordering};

use crate::Result;
use crate::execution::Failure;

static CHILD_GROUP: AtomicI32 = AtomicI32::new(0);
static INTERRUPTED: AtomicI32 = AtomicI32::new(0);
const KINDS: [i32; 3] = [1, 2, 15];

unsafe extern "C" {
    fn signal(kind: i32, handler: usize) -> usize;
    fn kill(pid: i32, kind: i32) -> i32;
}

#[derive(bevy_ecs::prelude::Resource)]
pub struct Signals {
    previous: [usize; 3],
    installed: usize,
}

impl Signals {
    pub fn alive(pid: u32) -> bool {
        pid > 0 && pid <= i32::MAX as u32 && unsafe { kill(pid as i32, 0) } == 0
    }

    pub fn install() -> Result<Self> {
        CHILD_GROUP.store(0, Ordering::Relaxed);
        INTERRUPTED.store(0, Ordering::Relaxed);
        let mut scope = Signals {
            previous: [0; 3],
            installed: 0,
        };
        for (index, kind) in KINDS.into_iter().enumerate() {
            let previous = unsafe { signal(kind, Self::forward as *const () as usize) };
            if previous == usize::MAX {
                return Err(std::io::Error::last_os_error().into());
            }
            scope.previous[index] = previous;
            scope.installed += 1;
        }
        Ok(scope)
    }

    extern "C" fn forward(kind: i32) {
        INTERRUPTED.store(kind, Ordering::Relaxed);
        let group = CHILD_GROUP.load(Ordering::Relaxed);
        if group > 0 {
            unsafe {
                kill(-group, kind);
            }
        }
    }

    pub fn track(pid: u32) -> Group {
        CHILD_GROUP.store(pid as i32, Ordering::Relaxed);
        let pending = INTERRUPTED.load(Ordering::Relaxed);
        if pending != 0 {
            Self::forward(pending);
        }
        Group(pid)
    }

    pub fn terminate_group(pid: u32) {
        if pid > 0 && pid <= i32::MAX as u32 {
            unsafe {
                kill(-(pid as i32), 9);
            }
        }
    }

    pub fn pending() -> Option<i32> {
        let pending = INTERRUPTED.load(Ordering::Relaxed);
        if pending == 0 { None } else { Some(pending) }
    }

    pub fn check() -> Result<()> {
        let pending = INTERRUPTED.load(Ordering::Relaxed);
        if pending != 0 {
            return Err(Failure::interrupted(pending).into());
        }
        Ok(())
    }
}

impl Drop for Signals {
    fn drop(&mut self) {
        for (index, kind) in KINDS.into_iter().enumerate().take(self.installed) {
            unsafe {
                signal(kind, self.previous[index]);
            }
        }
    }
}

pub struct Group(u32);

impl Drop for Group {
    fn drop(&mut self) {
        Signals::terminate_group(self.0);
        let _ =
            CHILD_GROUP.compare_exchange(self.0 as i32, 0, Ordering::Relaxed, Ordering::Relaxed);
    }
}
