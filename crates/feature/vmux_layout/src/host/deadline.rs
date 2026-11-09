use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};

use bevy::prelude::*;
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};

const DEADLINE_COALESCE_WINDOW: std::time::Duration = std::time::Duration::from_millis(1);

#[derive(Resource, Clone, Default)]
pub(super) struct DeadlineScheduler {
    state: Arc<(Mutex<ScheduledDeadline>, Condvar)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum DeadlineOwner {
    LayoutAnimation(Entity),
    PaneZoomAnimation(Entity),
    StackAnimation(Entity),
    PageSlot(Entity),
    Prewarm(&'static str),
    Presentation(Entity),
}

#[derive(Default)]
struct ScheduledDeadline {
    deadlines: BTreeMap<std::time::Instant, HashSet<DeadlineOwner>>,
    owners: HashMap<DeadlineOwner, std::time::Instant>,
    proxy: Option<EventLoopProxy<WinitUserEvent>>,
    worker_started: bool,
}

impl DeadlineScheduler {
    pub(super) fn schedule(
        &self,
        owner: DeadlineOwner,
        deadline: std::time::Instant,
        proxy: Option<&EventLoopProxyWrapper>,
    ) {
        let Some(proxy) = proxy else {
            return;
        };
        let start_worker = {
            let (state, wake) = &*self.state;
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.owners.get(&owner) == Some(&deadline) {
                return;
            }
            let earliest = state.deadlines.keys().next().copied();
            state.remove(owner);
            state.deadlines.entry(deadline).or_default().insert(owner);
            state.owners.insert(owner, deadline);
            state.proxy = Some((**proxy).clone());
            let start_worker = !state.worker_started;
            state.worker_started = true;
            if earliest.is_none_or(|earliest| deadline < earliest) {
                wake.notify_one();
            }
            start_worker
        };
        if !start_worker {
            return;
        }
        let shared = self.state.clone();
        std::thread::spawn(move || Self::run(shared));
    }

    pub(super) fn cancel(&self, owner: DeadlineOwner) {
        let (state, wake) = &*self.state;
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let earliest = state.deadlines.keys().next().copied();
        state.remove(owner);
        if state.deadlines.keys().next().copied() != earliest {
            wake.notify_one();
        }
    }

    fn run(shared: Arc<(Mutex<ScheduledDeadline>, Condvar)>) {
        let (state, wake) = &*shared;
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            let Some(deadline) = state.deadlines.keys().next().copied() else {
                state = wake
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                continue;
            };
            let wake_at = deadline + DEADLINE_COALESCE_WINDOW;
            let delay = wake_at.saturating_duration_since(std::time::Instant::now());
            if !delay.is_zero() {
                let (next, timeout) = wake
                    .wait_timeout(state, delay)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state = next;
                if !timeout.timed_out() || state.deadlines.keys().next().copied() != Some(deadline)
                {
                    continue;
                }
            }
            let now = std::time::Instant::now();
            while state
                .deadlines
                .first_key_value()
                .is_some_and(|(deadline, _)| *deadline <= now)
            {
                let Some((deadline, owners)) = state.deadlines.pop_first() else {
                    break;
                };
                for owner in owners {
                    if state.owners.get(&owner) == Some(&deadline) {
                        state.owners.remove(&owner);
                    }
                }
            }
            let proxy = state.proxy.clone();
            drop(state);
            if let Some(proxy) = proxy {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }
            state = shared
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

impl ScheduledDeadline {
    fn remove(&mut self, owner: DeadlineOwner) {
        let Some(deadline) = self.owners.remove(&owner) else {
            return;
        };
        let Some(owners) = self.deadlines.get_mut(&deadline) else {
            return;
        };
        owners.remove(&owner);
        if owners.is_empty() {
            self.deadlines.remove(&deadline);
        }
    }
}
