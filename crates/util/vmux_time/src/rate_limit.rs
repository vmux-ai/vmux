use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;

use bevy_app::{App, Last, Plugin};
use bevy_ecs::prelude::*;
use web_time::{Duration, Instant};

use crate::{Clock, TimePlugin};

pub struct RateLimitPlugin<K: Eq + Hash + Send + Sync + 'static>(PhantomData<fn() -> K>);

impl<K: Eq + Hash + Send + Sync + 'static> Default for RateLimitPlugin<K> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<K: Eq + Hash + Send + Sync + 'static> Plugin for RateLimitPlugin<K> {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TimePlugin>() {
            app.add_plugins(TimePlugin);
        }
        if app.world().contains_resource::<Registered<K>>() {
            return;
        }
        app.insert_resource(Registered::<K>(PhantomData))
            .add_systems(Last, prune::<K>);
    }

    fn is_unique(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateLimitPolicy {
    burst: u32,
    refill_per_interval: u32,
    interval: Duration,
    max_entries: usize,
}

impl RateLimitPolicy {
    pub const DEFAULT_MAX_ENTRIES: usize = 10_000;

    pub const fn new(burst: u32, refill_per_interval: u32, interval: Duration) -> Self {
        Self {
            burst,
            refill_per_interval,
            interval,
            max_entries: Self::DEFAULT_MAX_ENTRIES,
        }
    }

    pub const fn per_second(burst: u32, per_second: u32) -> Self {
        Self::new(burst, per_second, Duration::from_secs(1))
    }

    pub const fn per_minute(burst: u32, per_minute: u32) -> Self {
        Self::new(burst, per_minute, Duration::from_secs(60))
    }

    pub const fn with_max_entries(mut self, max_entries: usize) -> Self {
        self.max_entries = max_entries;
        self
    }

    pub const fn burst(&self) -> u32 {
        self.burst
    }

    pub const fn refill_per_interval(&self) -> u32 {
        self.refill_per_interval
    }

    pub const fn interval(&self) -> Duration {
        self.interval
    }

    pub const fn max_entries(&self) -> usize {
        self.max_entries
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateLimitDecision {
    allowed: bool,
    retry_after: Duration,
}

impl RateLimitDecision {
    pub const fn allowed() -> Self {
        Self {
            allowed: true,
            retry_after: Duration::ZERO,
        }
    }

    pub const fn limited(retry_after: Duration) -> Self {
        Self {
            allowed: false,
            retry_after,
        }
    }

    pub const fn is_allowed(self) -> bool {
        self.allowed
    }

    pub const fn is_limited(self) -> bool {
        !self.allowed
    }

    pub const fn retry_after(self) -> Option<Duration> {
        if self.allowed {
            None
        } else {
            Some(self.retry_after)
        }
    }
}

#[derive(Component, Clone, Debug)]
pub struct RateLimiter<K: Eq + Hash + Send + Sync + 'static> {
    policy: RateLimitPolicy,
    buckets: HashMap<K, Bucket>,
}

impl<K: Eq + Hash + Send + Sync + 'static> RateLimiter<K> {
    pub fn new(policy: RateLimitPolicy) -> Self {
        Self {
            policy,
            buckets: HashMap::new(),
        }
    }

    pub const fn policy(&self) -> RateLimitPolicy {
        self.policy
    }

    pub fn set_policy(&mut self, policy: RateLimitPolicy) {
        self.policy = policy;
        if policy.max_entries == 0 {
            self.buckets.clear();
            return;
        }
        let interval_nanos = policy.interval.as_nanos();
        for bucket in self.buckets.values_mut() {
            bucket.tokens = bucket.tokens.min(policy.burst);
            if interval_nanos == 0 || bucket.tokens >= policy.burst {
                bucket.remainder = 0;
            } else {
                bucket.remainder = bucket.remainder.min(interval_nanos - 1);
            }
        }
    }

    pub fn tracked(&self) -> usize {
        self.buckets.len()
    }

    pub fn clear(&mut self) {
        self.buckets.clear();
    }

    pub fn check(&mut self, key: K, clock: &Clock) -> RateLimitDecision {
        self.check_cost(key, 1, clock)
    }

    pub fn check_cost(&mut self, key: K, cost: u32, clock: &Clock) -> RateLimitDecision {
        if cost == 0 {
            return RateLimitDecision::allowed();
        }
        if cost > self.policy.burst {
            return RateLimitDecision::limited(Duration::MAX);
        }
        if self.policy.max_entries == 0 {
            return RateLimitDecision::allowed();
        }
        if self.policy.interval.is_zero() && self.policy.refill_per_interval > 0 {
            return RateLimitDecision::allowed();
        }
        let now = clock.now();
        if let Some(bucket) = self.buckets.get_mut(&key) {
            bucket.refill(now, self.policy);
            if bucket.tokens >= cost {
                bucket.tokens -= cost;
                return RateLimitDecision::allowed();
            }
            return RateLimitDecision::limited(bucket.retry_after(cost, self.policy));
        }
        if self.buckets.len() >= self.policy.max_entries {
            self.evict_idle_at(now);
            if self.buckets.len() >= self.policy.max_entries {
                return RateLimitDecision::allowed();
            }
        }
        self.buckets.insert(
            key,
            Bucket {
                tokens: self.policy.burst - cost,
                last_refill: now,
                remainder: 0,
            },
        );
        RateLimitDecision::allowed()
    }

    pub fn peek(&self, key: &K, clock: &Clock) -> RateLimitDecision {
        self.peek_cost(key, 1, clock)
    }

    pub fn peek_cost(&self, key: &K, cost: u32, clock: &Clock) -> RateLimitDecision {
        if cost == 0 {
            return RateLimitDecision::allowed();
        }
        if cost > self.policy.burst {
            return RateLimitDecision::limited(Duration::MAX);
        }
        if self.policy.max_entries == 0 {
            return RateLimitDecision::allowed();
        }
        if self.policy.interval.is_zero() && self.policy.refill_per_interval > 0 {
            return RateLimitDecision::allowed();
        }
        let Some(bucket) = self.buckets.get(key) else {
            return RateLimitDecision::allowed();
        };
        let mut snapshot = *bucket;
        snapshot.refill(clock.now(), self.policy);
        if snapshot.tokens >= cost {
            RateLimitDecision::allowed()
        } else {
            RateLimitDecision::limited(snapshot.retry_after(cost, self.policy))
        }
    }

    pub fn evict_idle(&mut self, clock: &Clock) {
        if self.buckets.is_empty() {
            return;
        }
        self.evict_idle_at(clock.now());
    }

    fn has_idle_at(&self, now: Instant) -> bool {
        self.buckets
            .values()
            .any(|bucket| bucket.is_idle(now, self.policy))
    }

    fn evict_idle_at(&mut self, now: Instant) {
        let policy = self.policy;
        self.buckets
            .retain(|_, bucket| !bucket.is_idle(now, policy));
    }
}

#[derive(Resource)]
struct Registered<K: Send + Sync + 'static>(PhantomData<fn() -> K>);

#[derive(Clone, Copy, Debug)]
struct Bucket {
    tokens: u32,
    last_refill: Instant,
    remainder: u128,
}

impl Bucket {
    fn refill(&mut self, now: Instant, policy: RateLimitPolicy) {
        if self.tokens >= policy.burst {
            self.tokens = policy.burst;
            self.remainder = 0;
            if now > self.last_refill {
                self.last_refill = now;
            }
            return;
        }
        if policy.refill_per_interval == 0 {
            if now > self.last_refill {
                self.last_refill = now;
            }
            return;
        }
        let interval_nanos = policy.interval.as_nanos();
        if interval_nanos == 0 {
            self.tokens = policy.burst;
            self.remainder = 0;
            if now > self.last_refill {
                self.last_refill = now;
            }
            return;
        }
        let elapsed_nanos = now.saturating_duration_since(self.last_refill).as_nanos();
        if elapsed_nanos == 0 {
            return;
        }
        let rate = u128::from(policy.refill_per_interval);
        let total = self
            .remainder
            .saturating_add(elapsed_nanos.saturating_mul(rate));
        let gained = total / interval_nanos;
        let missing = u128::from(policy.burst - self.tokens);
        if gained >= missing {
            self.tokens = policy.burst;
            self.remainder = 0;
        } else {
            self.tokens += gained as u32;
            self.remainder = total % interval_nanos;
        }
        self.last_refill = now;
    }

    fn retry_after(&self, cost: u32, policy: RateLimitPolicy) -> Duration {
        if self.tokens >= cost {
            return Duration::ZERO;
        }
        if cost > policy.burst || policy.refill_per_interval == 0 {
            return Duration::MAX;
        }
        let interval_nanos = policy.interval.as_nanos();
        if interval_nanos == 0 {
            return Duration::ZERO;
        }
        let deficit = u128::from(cost - self.tokens);
        let needed_units = deficit
            .saturating_mul(interval_nanos)
            .saturating_sub(self.remainder);
        let wait_nanos = needed_units.div_ceil(u128::from(policy.refill_per_interval));
        Self::duration_from_nanos_saturating(wait_nanos)
    }

    fn is_idle(&self, now: Instant, policy: RateLimitPolicy) -> bool {
        let mut current = *self;
        current.refill(now, policy);
        current.tokens >= policy.burst
    }

    fn duration_from_nanos_saturating(nanos: u128) -> Duration {
        let secs = nanos / 1_000_000_000;
        if secs > u128::from(u64::MAX) {
            return Duration::MAX;
        }
        let subsec_nanos = (nanos % 1_000_000_000) as u32;
        Duration::new(secs as u64, subsec_nanos)
    }
}

fn prune<K: Eq + Hash + Send + Sync + 'static>(
    clock: Res<Clock>,
    mut limiters: Query<&mut RateLimiter<K>>,
) {
    let mut now = None;
    for mut limiter in &mut limiters {
        if limiter.buckets.is_empty() {
            continue;
        }
        let current = *now.get_or_insert_with(|| clock.now());
        if limiter.has_idle_at(current) {
            limiter.evict_idle_at(current);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_exhaustion_and_deterministic_fractional_refill() {
        let clock = Clock::fake(1_000);
        let mut limiter = RateLimiter::new(RateLimitPolicy::per_second(2, 3));

        assert_eq!(limiter.check("a", &clock), RateLimitDecision::allowed());
        assert_eq!(limiter.check("a", &clock), RateLimitDecision::allowed());
        let limited = limiter.check("a", &clock);
        assert!(limited.is_limited());
        assert_eq!(
            limited.retry_after(),
            Some(Duration::from_nanos(333_333_334))
        );

        for _ in 0..3 {
            clock.advance(Duration::from_millis(100));
            assert!(limiter.peek(&"a", &clock).is_limited());
        }
        clock.advance(Duration::from_nanos(33_333_333));
        let almost = limiter.check("a", &clock);
        assert_eq!(almost.retry_after(), Some(Duration::from_nanos(1)));

        clock.advance(Duration::from_nanos(1));
        assert!(limiter.peek(&"a", &clock).is_allowed());
        assert!(limiter.check("a", &clock).is_allowed());
        assert!(limiter.check("a", &clock).is_limited());

        clock.advance(Duration::from_nanos(333_333_333));
        assert!(limiter.check("a", &clock).is_allowed());
        clock.advance(Duration::from_nanos(333_333_333));
        assert!(limiter.check("a", &clock).is_allowed());
    }

    #[test]
    fn retry_after_reports_exact_remaining_wait_until_next_token() {
        let clock = Clock::fake(1_000);
        let mut limiter = RateLimiter::new(RateLimitPolicy::per_minute(1, 60));

        assert!(limiter.check("client", &clock).is_allowed());
        assert_eq!(
            limiter.check("client", &clock).retry_after(),
            Some(Duration::from_secs(1))
        );

        clock.advance(Duration::from_millis(250));
        assert_eq!(
            limiter.peek(&"client", &clock).retry_after(),
            Some(Duration::from_millis(750))
        );
        clock.advance(Duration::from_millis(749));
        assert_eq!(
            limiter.check("client", &clock).retry_after(),
            Some(Duration::from_millis(1))
        );
        clock.advance(Duration::from_millis(1));
        assert!(limiter.check("client", &clock).is_allowed());
    }

    #[test]
    fn independent_keys_do_not_interfere() {
        let clock = Clock::fake(1_000);
        let mut limiter = RateLimiter::new(RateLimitPolicy::per_second(1, 1));

        assert!(limiter.check("alpha", &clock).is_allowed());
        assert!(limiter.check("alpha", &clock).is_limited());
        assert!(limiter.check("beta", &clock).is_allowed());
        assert!(limiter.check("beta", &clock).is_limited());
        assert_eq!(limiter.tracked(), 2);

        clock.advance(Duration::from_secs(1));
        assert!(limiter.check("alpha", &clock).is_allowed());
        assert!(limiter.peek(&"beta", &clock).is_allowed());
    }

    #[test]
    fn max_entries_evicts_idle_buckets_first_then_fails_open() {
        let clock = Clock::fake(1_000);
        let mut limiter = RateLimiter::new(RateLimitPolicy::per_second(2, 2).with_max_entries(2));

        assert!(limiter.check("k1", &clock).is_allowed());
        assert!(limiter.check("k2", &clock).is_allowed());
        assert_eq!(limiter.tracked(), 2);

        assert!(limiter.check("k3", &clock).is_allowed());
        assert_eq!(limiter.tracked(), 2);
        assert!(limiter.check("k3", &clock).is_allowed());
        assert!(limiter.check("k3", &clock).is_allowed());
        assert_eq!(limiter.tracked(), 2);

        clock.advance(Duration::from_millis(500));
        assert!(limiter.check("k3", &clock).is_allowed());
        assert_eq!(limiter.tracked(), 1);
        assert!(limiter.check("k3", &clock).is_allowed());
        assert!(limiter.check("k3", &clock).is_limited());

        limiter.set_policy(RateLimitPolicy::per_second(2, 2).with_max_entries(0));
        assert_eq!(limiter.tracked(), 0);
        assert!(limiter.check("k3", &clock).is_allowed());
        assert_eq!(limiter.tracked(), 0);
    }

    #[test]
    fn check_cost_handles_multi_token_requests_without_partial_consumption() {
        let clock = Clock::fake(1_000);
        let mut limiter = RateLimiter::new(RateLimitPolicy::per_second(5, 2));

        assert_eq!(
            limiter.check_cost("job", 6, &clock),
            RateLimitDecision::limited(Duration::MAX)
        );
        assert_eq!(limiter.tracked(), 0);

        assert!(limiter.check_cost("job", 3, &clock).is_allowed());
        let refused = limiter.check_cost("job", 4, &clock);
        assert_eq!(refused, RateLimitDecision::limited(Duration::from_secs(1)));

        assert!(limiter.check_cost("job", 2, &clock).is_allowed());
        assert!(limiter.check_cost("job", 0, &clock).is_allowed());
        assert_eq!(
            limiter.check_cost("job", 1, &clock),
            RateLimitDecision::limited(Duration::from_millis(500))
        );

        clock.advance(Duration::from_secs(1));
        assert!(limiter.check_cost("job", 2, &clock).is_allowed());
    }

    #[test]
    fn zero_refill_enforces_fixed_burst_without_idle_eviction() {
        let clock = Clock::fake(1_000);
        let mut limiter = RateLimiter::new(RateLimitPolicy::per_second(2, 0));

        assert!(limiter.check("key", &clock).is_allowed());
        assert!(limiter.check("key", &clock).is_allowed());
        assert_eq!(
            limiter.check("key", &clock),
            RateLimitDecision::limited(Duration::MAX)
        );

        clock.advance(Duration::from_secs(3600));
        limiter.evict_idle(&clock);
        assert_eq!(limiter.tracked(), 1);
        assert_eq!(
            limiter.check("key", &clock),
            RateLimitDecision::limited(Duration::MAX)
        );

        limiter.clear();
        assert_eq!(limiter.tracked(), 0);
        assert!(limiter.check("key", &clock).is_allowed());
    }

    #[test]
    fn plugin_is_idempotent_and_prunes_idle_buckets_automatically() {
        let clock = Clock::fake(1_000);
        let mut app = App::new();
        app.insert_resource(clock.clone()).add_plugins((
            RateLimitPlugin::<String>::default(),
            RateLimitPlugin::<String>::default(),
        ));

        let entity = app
            .world_mut()
            .spawn(RateLimiter::<String>::new(RateLimitPolicy::per_second(
                2, 1,
            )))
            .id();

        {
            let mut limiter = app
                .world_mut()
                .get_mut::<RateLimiter<String>>(entity)
                .unwrap();
            assert!(limiter.check("first".to_string(), &clock).is_allowed());
            assert!(limiter.check("second".to_string(), &clock).is_allowed());
            assert!(limiter.check("second".to_string(), &clock).is_allowed());
            assert_eq!(limiter.tracked(), 2);
        }

        app.update();
        assert_eq!(
            app.world()
                .get::<RateLimiter<String>>(entity)
                .unwrap()
                .tracked(),
            2
        );

        clock.advance(Duration::from_secs(1));
        app.update();
        assert_eq!(
            app.world()
                .get::<RateLimiter<String>>(entity)
                .unwrap()
                .tracked(),
            1
        );

        clock.advance(Duration::from_secs(1));
        app.update();
        assert_eq!(
            app.world()
                .get::<RateLimiter<String>>(entity)
                .unwrap()
                .tracked(),
            0
        );
    }
}
