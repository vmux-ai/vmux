use std::marker::PhantomData;
use std::time::{Duration, Instant};

use bevy::prelude::*;

pub struct SearchPlugin<M> {
    debounce: Duration,
    marker: PhantomData<fn() -> M>,
}

impl<M> SearchPlugin<M> {
    pub fn new(debounce: Duration) -> Self {
        Self {
            debounce,
            marker: PhantomData,
        }
    }
}

impl<M: Send + Sync + 'static> Plugin for SearchPlugin<M> {
    fn build(&self, app: &mut App) {
        app.insert_resource(Debounce::<M>(self.debounce, PhantomData))
            .add_observer(request::<M>)
            .add_observer(cancel::<M>)
            .add_observer(cleanup::<M>)
            .add_systems(Update, publish::<M>)
            .add_systems(Last, wake::<M>);
    }
}

#[derive(Component)]
pub struct State<M> {
    query: Option<String>,
    revision: u64,
    marker: PhantomData<fn() -> M>,
}

impl<M> Default for State<M> {
    fn default() -> Self {
        Self {
            query: None,
            revision: 0,
            marker: PhantomData,
        }
    }
}

impl<M> State<M> {
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    pub fn matches(&self, revision: u64) -> bool {
        revision != 0 && self.revision == revision
    }

    fn request(&mut self, query: &str, force: bool) -> Option<u64> {
        if !force && self.query.as_deref() == Some(query) {
            return None;
        }
        self.query = Some(query.to_string());
        self.revision = self.revision.wrapping_add(1).max(1);
        Some(self.revision)
    }

    fn cancel(&mut self) {
        self.query = None;
        self.revision = self.revision.wrapping_add(1).max(1);
    }
}

#[derive(EntityEvent)]
pub struct Request<M: Send + Sync + 'static> {
    #[event_target]
    pub target: Entity,
    pub query: String,
    force: bool,
    marker: PhantomData<fn() -> M>,
}

impl<M: Send + Sync + 'static> Request<M> {
    pub fn new(target: Entity, query: String) -> Self {
        Self {
            target,
            query,
            force: false,
            marker: PhantomData,
        }
    }

    pub fn forced(target: Entity, query: String) -> Self {
        Self {
            target,
            query,
            force: true,
            marker: PhantomData,
        }
    }
}

#[derive(EntityEvent)]
pub struct Cancel<M: Send + Sync + 'static> {
    #[event_target]
    pub target: Entity,
    marker: PhantomData<fn() -> M>,
}

impl<M: Send + Sync + 'static> Cancel<M> {
    pub fn new(target: Entity) -> Self {
        Self {
            target,
            marker: PhantomData,
        }
    }
}

#[derive(EntityEvent)]
pub struct Ready<M: Send + Sync + 'static> {
    #[event_target]
    pub target: Entity,
    pub query: String,
    pub revision: u64,
    marker: PhantomData<fn() -> M>,
}

#[derive(Resource)]
struct Debounce<M>(Duration, PhantomData<fn() -> M>);

#[derive(Component)]
struct Pending<M> {
    query: String,
    revision: u64,
    due: Instant,
    marker: PhantomData<fn() -> M>,
}

fn request<M: Send + Sync + 'static>(
    trigger: On<Request<M>>,
    debounce: Res<Debounce<M>>,
    mut states: Query<&mut State<M>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok(mut state) = states.get_mut(event.target) else {
        return;
    };
    let Some(revision) = state.request(&event.query, event.force) else {
        return;
    };
    if debounce.0.is_zero() {
        commands.entity(event.target).remove::<Pending<M>>();
        commands.trigger(Ready::<M> {
            target: event.target,
            query: event.query.clone(),
            revision,
            marker: PhantomData,
        });
        return;
    }
    commands.entity(event.target).insert(Pending::<M> {
        query: event.query.clone(),
        revision,
        due: Instant::now() + debounce.0,
        marker: PhantomData,
    });
}

fn cancel<M: Send + Sync + 'static>(
    trigger: On<Cancel<M>>,
    mut states: Query<&mut State<M>>,
    mut commands: Commands,
) {
    let target = trigger.event().target;
    if let Ok(mut state) = states.get_mut(target) {
        state.cancel();
    }
    commands.entity(target).remove::<Pending<M>>();
}

fn cleanup<M: Send + Sync + 'static>(trigger: On<Remove, State<M>>, mut commands: Commands) {
    commands
        .entity(trigger.event_target())
        .remove::<Pending<M>>();
}

fn publish<M: Send + Sync + 'static>(
    pending: Query<(Entity, &State<M>, &Pending<M>)>,
    mut commands: Commands,
) {
    let now = Instant::now();
    for (target, state, pending) in &pending {
        if now < pending.due {
            continue;
        }
        let ready =
            state.matches(pending.revision) && state.query() == Some(pending.query.as_str());
        let query = pending.query.clone();
        let revision = pending.revision;
        commands.entity(target).remove::<Pending<M>>();
        if ready {
            commands.trigger(Ready::<M> {
                target,
                query,
                revision,
                marker: PhantomData,
            });
        }
    }
}

fn wake<M: Send + Sync + 'static>(
    pending: Query<(), With<Pending<M>>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    if pending.is_empty() {
        return;
    }
    let Some(proxy) = proxy else {
        return;
    };
    let _ = (**proxy).send_event(bevy::winit::WinitUserEvent::WakeUp);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Catalog;

    #[derive(Component, Default)]
    struct Captured(Vec<(String, u64)>);

    fn capture(trigger: On<Ready<Catalog>>, mut captured: Single<&mut Captured>) {
        captured
            .0
            .push((trigger.event().query.clone(), trigger.event().revision));
    }

    #[test]
    fn latest_request_replaces_pending_work() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(SearchPlugin::<Catalog>::new(Duration::from_secs(1)))
            .add_observer(capture);
        let target = app
            .world_mut()
            .spawn((State::<Catalog>::default(), Captured::default()))
            .id();

        app.world_mut()
            .trigger(Request::<Catalog>::new(target, "old".to_string()));
        app.world_mut()
            .trigger(Request::<Catalog>::new(target, "latest".to_string()));
        app.update();

        let pending = app.world().get::<Pending<Catalog>>(target).unwrap();
        let state = app.world().get::<State<Catalog>>(target).unwrap();
        let captured = app.world().get::<Captured>(target).unwrap();
        assert_eq!(pending.query, "latest");
        assert!(state.matches(pending.revision));
        assert!(captured.0.is_empty());
    }

    #[test]
    fn duplicate_requests_are_suppressed() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(SearchPlugin::<Catalog>::new(Duration::ZERO))
            .add_observer(capture);
        let target = app
            .world_mut()
            .spawn((State::<Catalog>::default(), Captured::default()))
            .id();

        app.world_mut()
            .trigger(Request::<Catalog>::new(target, "same".to_string()));
        app.world_mut()
            .trigger(Request::<Catalog>::new(target, "same".to_string()));
        app.update();

        assert_eq!(app.world().get::<Captured>(target).unwrap().0.len(), 1);
    }

    #[test]
    fn cancel_invalidates_pending_results() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(SearchPlugin::<Catalog>::new(Duration::from_secs(1)))
            .add_observer(capture);
        let target = app
            .world_mut()
            .spawn((State::<Catalog>::default(), Captured::default()))
            .id();

        app.world_mut()
            .trigger(Request::<Catalog>::new(target, "query".to_string()));
        app.world_mut().trigger(Cancel::<Catalog>::new(target));
        app.update();

        assert!(app.world().get::<Captured>(target).unwrap().0.is_empty());
    }
}
