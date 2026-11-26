use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::task::{Wake, Waker};
use std::time::Duration;

use bevy_app::App;
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::time::{
    BackoffPolicy, Clock, Deadline, DeadlinePlugin, DeadlineReached, Debounce, RateLimitDecision,
    RateLimitPlugin, RateLimitPolicy, RateLimiter, RetryBackoff, TimeWake,
};
use vmux_app::ecs::{
    ArchiveRequest, ArchivedAt, CreatedAt, ExpiresAt, JobWake, Touch, UnarchiveRequest, UpdatedAt,
};

struct Refresh;
struct Autosave;
struct Reconnect;

#[derive(Resource, Default)]
struct Fired {
    refresh: Vec<Entity>,
    autosave: Vec<Entity>,
    reconnect: Vec<Entity>,
}

fn on_refresh(reached: On<DeadlineReached<Refresh>>, mut fired: ResMut<Fired>) {
    fired.refresh.push(reached.entity);
}

fn on_autosave(reached: On<DeadlineReached<Autosave>>, mut fired: ResMut<Fired>) {
    fired.autosave.push(reached.entity);
}

fn on_reconnect(reached: On<DeadlineReached<Reconnect>>, mut fired: ResMut<Fired>) {
    fired.reconnect.push(reached.entity);
}

struct WakeCounter {
    count: Arc<AtomicUsize>,
    tx: mpsc::SyncSender<()>,
}

impl Wake for WakeCounter {
    fn wake(self: Arc<Self>) {
        self.count.fetch_add(1, Ordering::SeqCst);
        let _ = self.tx.try_send(());
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.count.fetch_add(1, Ordering::SeqCst);
        let _ = self.tx.try_send(());
    }
}

#[test]
fn clock_deadlines_debounce_retry_and_timestamps_work_deterministically_through_vmux_app() {
    let clock = Clock::fake(1_700_000_000_000);
    let mut app = App::new();
    app.insert_resource(clock.clone())
        .init_resource::<Fired>()
        .add_plugins((
            VmuxPlugin::builder().jobs(true).build(),
            DeadlinePlugin::<Refresh>::default(),
            DeadlinePlugin::<Autosave>::default(),
            DeadlinePlugin::<Reconnect>::default(),
        ))
        .add_observer(on_refresh)
        .add_observer(on_autosave)
        .add_observer(on_reconnect);

    let root = app.world_mut().spawn(CreatedAt::default()).id();
    let child = app
        .world_mut()
        .spawn((
            ChildOf(root),
            CreatedAt::default(),
            Deadline::<Refresh>::after(&clock, Duration::from_secs(2)),
            Debounce::<Autosave>::new(Duration::from_secs(3)),
            RetryBackoff::<Reconnect>::scheduled(
                BackoffPolicy::exponential(Duration::from_secs(1), Duration::from_secs(3))
                    .with_max_attempts(3),
                &clock,
            ),
            ExpiresAt::after(&clock, Duration::from_secs(10)),
        ))
        .id();
    assert_eq!(
        app.world().get::<CreatedAt>(root),
        Some(&CreatedAt(1_700_000_000_000))
    );
    app.update();

    clock.advance(Duration::from_secs(1));
    app.update();
    assert_eq!(app.world().resource::<Fired>().reconnect, vec![child]);
    assert!(app.world().resource::<Fired>().refresh.is_empty());
    assert!(app.world().resource::<Fired>().autosave.is_empty());

    app.world_mut()
        .get_mut::<RetryBackoff<Reconnect>>(child)
        .unwrap()
        .record_failure(&clock);
    app.world_mut()
        .get_mut::<Debounce<Autosave>>(child)
        .unwrap()
        .reset(&clock);
    app.world_mut().trigger(Touch::with_ancestors(child));
    app.world_mut().trigger(ArchiveRequest { entity: child });
    app.update();

    assert_eq!(
        app.world().get::<UpdatedAt>(root),
        Some(&UpdatedAt(1_700_000_001_000))
    );
    assert_eq!(
        app.world().get::<ArchivedAt>(child),
        Some(&ArchivedAt(1_700_000_001_000))
    );
    app.world_mut().trigger(UnarchiveRequest { entity: child });
    app.update();
    assert!(app.world().get::<ArchivedAt>(child).is_none());

    clock.advance(Duration::from_secs(1));
    app.update();
    assert_eq!(app.world().resource::<Fired>().refresh, vec![child]);
    assert!(app.world().resource::<Fired>().autosave.is_empty());
    assert_eq!(app.world().resource::<Fired>().reconnect.len(), 1);

    clock.advance(Duration::from_secs(1));
    app.update();
    assert_eq!(app.world().resource::<Fired>().reconnect.len(), 2);
    assert!(app.world().resource::<Fired>().autosave.is_empty());

    clock.advance(Duration::from_secs(1));
    app.update();
    assert_eq!(app.world().resource::<Fired>().autosave, vec![child]);

    clock.advance(Duration::from_secs(6));
    app.update();
    assert!(app.world().get_entity(child).is_err());
    assert!(app.world().get_entity(root).is_ok());
}

#[test]
fn job_wake_automatically_wires_time_wake_for_coalesced_deadlines() {
    let mut app = App::new();
    app.add_plugins((
        VmuxPlugin::builder().jobs(true).build(),
        DeadlinePlugin::<Refresh>::default(),
    ));
    let count = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = mpsc::sync_channel(4);
    app.insert_resource(JobWake(Waker::from(Arc::new(WakeCounter {
        count: Arc::clone(&count),
        tx,
    }))));
    app.update();
    assert!(app.world().contains_resource::<TimeWake>());

    let clock = app.world().resource::<Clock>().clone();
    app.world_mut()
        .spawn(Deadline::<Refresh>::after(&clock, Duration::from_secs(20)));
    app.world_mut().spawn(Deadline::<Refresh>::after(
        &clock,
        Duration::from_millis(20),
    ));

    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn rate_limiter_works_deterministically_with_fake_clock_through_vmux_app() {
    let clock = Clock::fake(1_700_000_000_000);
    let mut app = App::new();
    app.insert_resource(clock.clone()).add_plugins((
        VmuxPlugin::builder().build(),
        RateLimitPlugin::<String>::default(),
        RateLimitPlugin::<String>::default(),
    ));

    let endpoint = app
        .world_mut()
        .spawn(RateLimiter::<String>::new(
            RateLimitPolicy::per_second(2, 2).with_max_entries(2),
        ))
        .id();

    {
        let mut limiter = app
            .world_mut()
            .get_mut::<RateLimiter<String>>(endpoint)
            .unwrap();
        assert_eq!(
            limiter.check("10.0.0.1".to_string(), &clock),
            RateLimitDecision::allowed()
        );
        assert_eq!(
            limiter.check("10.0.0.1".to_string(), &clock),
            RateLimitDecision::allowed()
        );
        assert_eq!(
            limiter.check("10.0.0.1".to_string(), &clock),
            RateLimitDecision::limited(Duration::from_millis(500))
        );
        assert_eq!(limiter.tracked(), 1);
    }

    clock.advance(Duration::from_millis(250));
    app.update();
    assert_eq!(
        app.world()
            .get::<RateLimiter<String>>(endpoint)
            .unwrap()
            .peek(&"10.0.0.1".to_string(), &clock),
        RateLimitDecision::limited(Duration::from_millis(250))
    );

    clock.advance(Duration::from_millis(250));
    {
        let mut limiter = app
            .world_mut()
            .get_mut::<RateLimiter<String>>(endpoint)
            .unwrap();
        assert!(limiter.check("10.0.0.1".to_string(), &clock).is_allowed());
        assert!(limiter.check("10.0.0.1".to_string(), &clock).is_limited());
    }

    clock.advance(Duration::from_secs(1));
    app.update();
    assert_eq!(
        app.world()
            .get::<RateLimiter<String>>(endpoint)
            .unwrap()
            .tracked(),
        0
    );
}
