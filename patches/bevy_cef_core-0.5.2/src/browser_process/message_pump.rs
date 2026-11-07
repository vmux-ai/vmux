use bevy::prelude::{Deref, Resource};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

const MAX_TIMER_DELAY_MS: u64 = 1000 / 30;
const DEFAULT_WAKE_MIN_INTERVAL: Duration = Duration::from_nanos(8_333_333);
const INTERACTION_WAKE_MIN_INTERVAL: Duration = Duration::from_millis(16);
const INTERACTION_WAKE_DURATION: Duration = Duration::from_millis(250);

#[repr(transparent)]
#[derive(Debug, Deref)]
pub struct MessageLoopWorkingReceiver(pub Receiver<MessageLoopTimer>);

#[derive(Debug, Clone, Copy)]
pub struct MessageLoopTimer(Instant);

impl MessageLoopTimer {
    pub fn new(delay_ms: i64) -> Self {
        let fire_time = if delay_ms <= 0 {
            Instant::now()
        } else {
            Instant::now() + Duration::from_millis(delay_ms as u64)
        };
        Self(fire_time)
    }

    #[inline]
    pub fn is_finished(&self) -> bool {
        self.0 <= Instant::now()
    }

    #[inline]
    pub fn fire_time(&self) -> Instant {
        self.0
    }

    #[inline]
    pub fn earliest(self, other: Self) -> Self {
        if self.0 <= other.0 { self } else { other }
    }
}

#[derive(Debug, Deref)]
pub struct MessageLoopWorkingMaxDelayTimer(MessageLoopTimer);

impl Default for MessageLoopWorkingMaxDelayTimer {
    fn default() -> Self {
        Self(MessageLoopTimer::new(MAX_TIMER_DELAY_MS as i64))
    }
}

#[derive(Debug)]
struct MessageLoopWakeState {
    started_at: Instant,
    min_interval_nanos: AtomicU64,
    interaction_until_nanos: AtomicU64,
}

#[derive(Resource, Clone, Debug)]
pub struct MessageLoopWakePolicy(Arc<MessageLoopWakeState>);

impl Default for MessageLoopWakePolicy {
    fn default() -> Self {
        Self(Arc::new(MessageLoopWakeState {
            started_at: Instant::now(),
            min_interval_nanos: AtomicU64::new(duration_nanos(DEFAULT_WAKE_MIN_INTERVAL)),
            interaction_until_nanos: AtomicU64::new(0),
        }))
    }
}

impl MessageLoopWakePolicy {
    pub fn set_min_wake_interval(&self, interval: Duration) {
        self.0
            .min_interval_nanos
            .store(duration_nanos(interval), Ordering::Relaxed);
    }

    pub fn begin_interaction(&self) {
        let elapsed = duration_nanos(self.0.started_at.elapsed());
        let deadline = elapsed.saturating_add(duration_nanos(INTERACTION_WAKE_DURATION));
        self.0
            .interaction_until_nanos
            .fetch_max(deadline, Ordering::Relaxed);
    }

    pub fn min_wake_interval(&self) -> Duration {
        let configured = Duration::from_nanos(self.0.min_interval_nanos.load(Ordering::Relaxed));
        let elapsed = duration_nanos(self.0.started_at.elapsed());
        let interaction_until = self.0.interaction_until_nanos.load(Ordering::Relaxed);
        if elapsed < interaction_until {
            return configured.min(INTERACTION_WAKE_MIN_INTERVAL);
        }
        configured
    }
}

fn duration_nanos(duration: Duration) -> u64 {
    duration.as_nanos().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_policy_defaults_to_120hz() {
        assert_eq!(
            MessageLoopWakePolicy::default().min_wake_interval(),
            Duration::from_nanos(8_333_333)
        );
    }

    #[test]
    fn wake_policy_can_be_throttled_for_background() {
        let policy = MessageLoopWakePolicy::default();

        policy.set_min_wake_interval(Duration::from_secs(1));

        assert_eq!(policy.min_wake_interval(), Duration::from_secs(1));
    }

    #[test]
    fn wake_policy_stays_responsive_during_interaction() {
        let policy = MessageLoopWakePolicy::default();
        policy.set_min_wake_interval(Duration::from_millis(100));

        policy.begin_interaction();

        assert_eq!(policy.min_wake_interval(), INTERACTION_WAKE_MIN_INTERVAL);
    }

    #[test]
    fn message_loop_timer_keeps_earliest_deadline() {
        let later = MessageLoopTimer::new(100);
        let earlier = MessageLoopTimer::new(0);

        assert_eq!(later.earliest(earlier).fire_time(), earlier.fire_time());
    }
}
