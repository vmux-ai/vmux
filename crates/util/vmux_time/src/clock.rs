use std::sync::{Arc, Mutex};

use bevy_ecs::prelude::Resource;
use web_time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Resource, Clone, Default)]
pub struct Clock {
    fake: Option<Arc<Mutex<FakeClock>>>,
}

#[derive(Clone, Copy, Debug)]
struct FakeClock {
    monotonic: Instant,
    unix_millis: i64,
    sub_millis_nanos: u32,
}

impl Clock {
    pub fn real() -> Self {
        Self { fake: None }
    }

    pub fn fake(unix_millis: i64) -> Self {
        Self::fake_at(Instant::now(), unix_millis)
    }

    pub fn fake_at(monotonic: Instant, unix_millis: i64) -> Self {
        Self {
            fake: Some(Arc::new(Mutex::new(FakeClock {
                monotonic,
                unix_millis,
                sub_millis_nanos: 0,
            }))),
        }
    }

    pub fn is_fake(&self) -> bool {
        self.fake.is_some()
    }

    pub fn now(&self) -> Instant {
        match &self.fake {
            Some(state) => {
                state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .monotonic
            }
            None => Instant::now(),
        }
    }

    pub fn unix_millis(&self) -> i64 {
        match &self.fake {
            Some(state) => {
                state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .unix_millis
            }
            None => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        }
    }

    pub fn elapsed_since(&self, earlier: Instant) -> Duration {
        self.now().saturating_duration_since(earlier)
    }

    pub fn deadline(&self, duration: Duration) -> Instant {
        self.now() + duration
    }

    pub fn advance(&self, duration: Duration) {
        let state = self
            .fake
            .as_ref()
            .expect("Clock::advance requires a fake clock");
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.monotonic += duration;
        let total_nanos = u128::from(state.sub_millis_nanos) + duration.as_nanos();
        let whole_millis = (total_nanos / 1_000_000).min(i64::MAX as u128) as i64;
        state.sub_millis_nanos = (total_nanos % 1_000_000) as u32;
        state.unix_millis = state.unix_millis.saturating_add(whole_millis);
    }

    pub fn set_unix_millis(&self, unix_millis: i64) {
        let state = self
            .fake
            .as_ref()
            .expect("Clock::set_unix_millis requires a fake clock");
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.unix_millis = unix_millis;
        state.sub_millis_nanos = 0;
    }

    pub fn set_now(&self, monotonic: Instant) {
        let state = self
            .fake
            .as_ref()
            .expect("Clock::set_now requires a fake clock");
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.monotonic = monotonic;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_clock_advances_monotonic_and_wall_time_deterministically() {
        let origin = Instant::now();
        let clock = Clock::fake_at(origin, 1_700_000_000_000);
        assert!(clock.is_fake());
        assert_eq!(clock.now(), origin);
        assert_eq!(clock.unix_millis(), 1_700_000_000_000);
        clock.advance(Duration::from_millis(250));
        assert_eq!(clock.elapsed_since(origin), Duration::from_millis(250));
        assert_eq!(clock.unix_millis(), 1_700_000_000_250);
        clock.set_unix_millis(1_800_000_000_000);
        assert_eq!(clock.unix_millis(), 1_800_000_000_000);
        assert_eq!(clock.elapsed_since(origin), Duration::from_millis(250));
    }

    #[test]
    fn fake_clock_accumulates_fractional_millisecond_advances() {
        let origin = Instant::now();
        let clock = Clock::fake_at(origin, 1_000);
        clock.advance(Duration::from_micros(500));
        assert_eq!(clock.unix_millis(), 1_000);
        assert_eq!(clock.elapsed_since(origin), Duration::from_micros(500));
        clock.advance(Duration::from_micros(500));
        assert_eq!(clock.unix_millis(), 1_001);
        assert_eq!(clock.elapsed_since(origin), Duration::from_millis(1));
        clock.advance(Duration::from_micros(700));
        clock.set_unix_millis(2_000);
        clock.advance(Duration::from_micros(500));
        assert_eq!(clock.unix_millis(), 2_000);
    }

    #[test]
    fn real_clock_reports_non_fake_and_nonzero_unix_millis() {
        let clock = Clock::real();
        assert!(!clock.is_fake());
        assert!(clock.unix_millis() > 0);
    }
}
