use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_api::protocol::AgentAttachment;
use vmux_ecs::{Order, PendingPrompt, PendingPromptAttachments};

use crate::Session;

pub struct QueuePlugin;

impl Plugin for QueuePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(enqueue).add_observer(remove);
    }
}

#[derive(Component, Debug, Default)]
pub struct PromptQueue {
    pub paused: bool,
    flush_pending: bool,
    next_id: u64,
    next_order: u32,
    revision: u64,
    root: Option<Entity>,
}

impl PromptQueue {
    pub fn flush_pending(&self) -> bool {
        self.flush_pending
    }

    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

#[derive(Component)]
struct Root;

#[derive(Component)]
pub struct QueuedPrompt {
    consumed: bool,
    echoed: bool,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueuedPromptId(pub u64);

#[derive(EntityEvent)]
struct Enqueue {
    #[event_target]
    session: Entity,
    text: String,
    attachments: Vec<AgentAttachment>,
    echoed: bool,
}

fn enqueue(
    event: On<Enqueue>,
    sessions: Query<(), With<Session>>,
    mut queues: Query<&mut PromptQueue>,
    roots: Query<&ChildOf, With<Root>>,
    mut items: Query<(Entity, &ChildOf, &QueuedPrompt, &mut Order)>,
    mut commands: Commands,
) {
    if !sessions.contains(event.session) {
        return;
    }
    let Ok(mut queue) = queues.get_mut(event.session) else {
        return;
    };
    let root = match queue.root {
        Some(root)
            if roots
                .get(root)
                .is_ok_and(|parent| parent.parent() == event.session) =>
        {
            root
        }
        _ => {
            let root = commands.spawn((Root, ChildOf(event.session))).id();
            queue.root = Some(root);
            root
        }
    };
    if queue.next_order == u32::MAX {
        let mut ordered = Vec::new();
        for (entity, parent, item, order) in &items {
            if parent.parent() == root && !item.consumed {
                ordered.push((order.0, entity));
            }
        }
        ordered.sort_unstable_by_key(|(order, _)| *order);
        let Ok(next_order) = u32::try_from(ordered.len()) else {
            return;
        };
        if next_order == u32::MAX {
            return;
        }
        for (index, (_, entity)) in ordered.into_iter().enumerate() {
            if let Ok((_, _, _, mut order)) = items.get_mut(entity) {
                order.0 = index as u32;
            }
        }
        queue.next_order = next_order;
    }
    let id = queue.next_id;
    let order = queue.next_order;
    queue.next_id = queue.next_id.wrapping_add(1);
    queue.next_order += 1;
    queue.paused = false;
    queue.changed();
    commands.spawn((
        ChildOf(root),
        QueuedPrompt {
            consumed: false,
            echoed: event.echoed,
        },
        QueuedPromptId(id),
        Order(order),
        PendingPrompt(event.text.clone()),
        PendingPromptAttachments(event.attachments.clone()),
    ));
}

fn remove(
    event: On<Remove, PromptQueue>,
    roots: Query<(Entity, &ChildOf), With<Root>>,
    mut commands: Commands,
) {
    for (entity, parent) in &roots {
        if parent.parent() == event.entity {
            commands.entity(entity).despawn();
        }
    }
}

#[derive(SystemParam)]
pub struct QueuedPrompts<'w, 's> {
    queues: Query<'w, 's, &'static mut PromptQueue>,
    sessions: Query<'w, 's, (), With<Session>>,
    roots: Query<'w, 's, &'static ChildOf, With<Root>>,
    items: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static QueuedPromptId,
            &'static mut QueuedPrompt,
            &'static Order,
            &'static PendingPrompt,
            &'static PendingPromptAttachments,
        ),
    >,
    commands: Commands<'w, 's>,
}

impl QueuedPrompts<'_, '_> {
    pub fn enqueue(
        &mut self,
        session: Entity,
        text: String,
        attachments: Vec<AgentAttachment>,
        echoed: bool,
    ) {
        if !self.sessions.contains(session) {
            return;
        }
        self.commands
            .entity(session)
            .entry::<PromptQueue>()
            .or_default();
        self.commands.trigger(Enqueue {
            session,
            text,
            attachments,
            echoed,
        });
    }

    pub fn ready(&self, session: Entity, idle: bool) -> bool {
        idle && self.queues.get(session).is_ok_and(|queue| !queue.paused) && !self.empty(session)
    }

    pub fn empty(&self, session: Entity) -> bool {
        let Some(root) = self.root(session) else {
            return true;
        };
        for (_, parent, _, item, _, _, _) in &self.items {
            if parent.parent() == root && !item.consumed {
                return false;
            }
        }
        true
    }

    pub fn request_flush(&mut self, session: Entity) -> bool {
        if self.empty(session) {
            return false;
        }
        let Ok(mut queue) = self.queues.get_mut(session) else {
            return false;
        };
        queue.paused = false;
        queue.flush_pending = true;
        queue.changed();
        true
    }

    pub fn flush_pending(&self, session: Entity) -> bool {
        self.queues
            .get(session)
            .is_ok_and(|queue| queue.flush_pending())
    }

    pub fn cancel_flush(&mut self, session: Entity) {
        if let Ok(mut queue) = self.queues.get_mut(session) {
            queue.flush_pending = false;
            queue.changed();
        }
    }

    pub fn resume(&mut self, session: Entity) {
        if let Ok(mut queue) = self.queues.get_mut(session) {
            queue.paused = false;
            queue.flush_pending = false;
            queue.changed();
        }
    }

    pub fn clear(&mut self, session: Entity) {
        let Some(root) = self.root(session) else {
            return;
        };
        for (entity, parent, _, mut item, _, _, _) in &mut self.items {
            if parent.parent() == root && !item.consumed {
                item.consumed = true;
                self.commands.entity(entity).despawn();
            }
        }
        self.resume(session);
    }

    pub fn remove(&mut self, session: Entity, id: u64) -> bool {
        let Some(root) = self.root(session) else {
            return false;
        };
        let mut removed = false;
        for (entity, parent, candidate, mut item, _, _, _) in &mut self.items {
            if parent.parent() == root && candidate.0 == id && !item.consumed {
                item.consumed = true;
                self.commands.entity(entity).despawn();
                removed = true;
                break;
            }
        }
        if !removed {
            return false;
        }
        if self.empty(session) {
            self.resume(session);
        } else if let Ok(mut queue) = self.queues.get_mut(session) {
            queue.changed();
        }
        true
    }

    pub fn take_next(&mut self, session: Entity) -> Option<(u64, String, Vec<AgentAttachment>)> {
        let root = self.root(session)?;
        let mut ordered = Vec::new();
        for (entity, parent, _, item, order, _, _) in &self.items {
            if parent.parent() == root && !item.consumed {
                ordered.push((order.0, entity));
            }
        }
        ordered.sort_unstable_by_key(|(order, _)| *order);
        let mut queue = self.queues.get_mut(session).ok()?;
        if queue.paused {
            return None;
        }
        let flush = queue.flush_pending;
        let mut first = None;
        let mut text = String::new();
        let mut attachments = Vec::new();
        for (_, entity) in ordered {
            let Ok((_, _, id, mut item, _, body, media)) = self.items.get_mut(entity) else {
                continue;
            };
            if item.consumed {
                continue;
            }
            first.get_or_insert(id.0);
            if !text.is_empty() && !body.0.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(&body.0);
            attachments.extend(media.0.iter().cloned());
            item.consumed = true;
            self.commands.entity(entity).despawn();
            if !flush {
                break;
            }
        }
        let first = first?;
        queue.flush_pending = false;
        queue.changed();
        Some((first, text, attachments))
    }

    fn root(&self, session: Entity) -> Option<Entity> {
        let root = self.queues.get(session).ok()?.root?;
        let parent = self.roots.get(root).ok()?;
        (parent.parent() == session).then_some(root)
    }
}

#[derive(SystemParam)]
pub struct QueuedPromptReads<'w, 's> {
    queues: Query<'w, 's, &'static PromptQueue>,
    roots: Query<'w, 's, &'static ChildOf, With<Root>>,
    items: Query<
        'w,
        's,
        (
            &'static ChildOf,
            &'static QueuedPromptId,
            &'static QueuedPrompt,
            &'static Order,
            &'static PendingPrompt,
            &'static PendingPromptAttachments,
        ),
    >,
}

impl QueuedPromptReads<'_, '_> {
    pub fn visible(&self, session: Entity) -> Vec<(u64, &str, &[AgentAttachment])> {
        let Some(root) = self.root(session) else {
            return Vec::new();
        };
        let mut ordered = Vec::new();
        for (parent, id, item, order, body, media) in &self.items {
            if parent.parent() == root && !item.consumed && !item.echoed {
                ordered.push((order.0, id.0, body.0.as_str(), media.0.as_slice()));
            }
        }
        ordered.sort_unstable_by_key(|(order, _, _, _)| *order);
        let mut rows = Vec::with_capacity(ordered.len());
        for (_, id, text, attachments) in ordered {
            rows.push((id, text, attachments));
        }
        rows
    }

    fn root(&self, session: Entity) -> Option<Entity> {
        let root = self.queues.get(session).ok()?.root?;
        let parent = self.roots.get(root).ok()?;
        (parent.parent() == session).then_some(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;

    struct QueueFixture {
        app: App,
        session: Entity,
    }

    impl QueueFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(QueuePlugin);
            let session = app
                .world_mut()
                .spawn((Session, PromptQueue::default()))
                .id();
            Self { app, session }
        }

        fn enqueue(&mut self, text: &str, attachments: Vec<AgentAttachment>, echoed: bool) {
            self.app.world_mut().trigger(Enqueue {
                session: self.session,
                text: text.into(),
                attachments,
                echoed,
            });
            self.app.world_mut().flush();
        }

        fn take(&mut self) -> Option<(u64, String, Vec<AgentAttachment>)> {
            let session = self.session;
            self.app
                .world_mut()
                .run_system_once(move |mut queues: QueuedPrompts| queues.take_next(session))
                .unwrap()
        }

        fn flush(&mut self) -> bool {
            let session = self.session;
            self.app
                .world_mut()
                .run_system_once(move |mut queues: QueuedPrompts| queues.request_flush(session))
                .unwrap()
        }

        fn visible(&mut self) -> Vec<(u64, String)> {
            let session = self.session;
            self.app
                .world_mut()
                .run_system_once(move |rows: QueuedPromptReads| {
                    let mut result = Vec::new();
                    for (id, text, _) in rows.visible(session) {
                        result.push((id, text.to_string()));
                    }
                    result
                })
                .unwrap()
        }
    }

    #[test]
    fn canonical_items_dispatch_once_in_fifo_order() {
        let mut fixture = QueueFixture::new();
        fixture.enqueue("first", Vec::new(), false);
        fixture.enqueue("second", Vec::new(), false);
        assert_eq!(fixture.take().unwrap().1, "first");
        assert_eq!(fixture.take().unwrap().1, "second");
        assert!(fixture.take().is_none());
        assert!(fixture.visible().is_empty());
    }

    #[test]
    fn pending_flush_includes_new_items_and_preserves_attachment_order() {
        let mut fixture = QueueFixture::new();
        let first = AgentAttachment {
            path: "/tmp/first.png".into(),
            name: "first".into(),
            mime_type: "image/png".into(),
            size: 1,
        };
        let second = AgentAttachment {
            path: "/tmp/second.png".into(),
            name: "second".into(),
            mime_type: "image/png".into(),
            size: 2,
        };
        fixture.enqueue("first", vec![first.clone()], false);
        assert!(fixture.flush());
        fixture.enqueue("second", vec![second.clone()], false);
        let (_, text, attachments) = fixture.take().unwrap();
        assert_eq!(text, "first\n\nsecond");
        assert_eq!(attachments, [first, second]);
        assert!(fixture.take().is_none());
    }

    #[test]
    fn initial_echo_is_hidden_only_from_the_queue_projection() {
        let mut fixture = QueueFixture::new();
        fixture.enqueue("initial", Vec::new(), true);
        assert!(fixture.visible().is_empty());
        assert_eq!(fixture.take().unwrap().1, "initial");
        assert!(fixture.take().is_none());
    }

    #[test]
    fn removing_queue_state_removes_only_its_prompt_children() {
        let mut fixture = QueueFixture::new();
        fixture.enqueue("pending", Vec::new(), false);
        let unrelated = fixture.app.world_mut().spawn(ChildOf(fixture.session)).id();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.session)
            .remove::<PromptQueue>();
        fixture.app.world_mut().flush();
        assert!(fixture.visible().is_empty());
        assert!(fixture.app.world().get_entity(unrelated).is_ok());
        assert!(fixture.take().is_none());
    }

    #[test]
    fn stable_id_removal_is_scoped_to_its_session() {
        let mut fixture = QueueFixture::new();
        fixture.enqueue("keep", Vec::new(), false);
        fixture.enqueue("remove", Vec::new(), false);
        let rows = fixture.visible();
        let removed_id = rows[1].0;
        let other = fixture
            .app
            .world_mut()
            .spawn((Session, PromptQueue::default()))
            .id();
        fixture.app.world_mut().trigger(Enqueue {
            session: other,
            text: "other".into(),
            attachments: Vec::new(),
            echoed: false,
        });
        fixture.app.world_mut().flush();
        let session = fixture.session;
        assert!(
            fixture
                .app
                .world_mut()
                .run_system_once(move |mut queues: QueuedPrompts| queues.remove(session, removed_id))
                .unwrap()
        );
        assert_eq!(fixture.visible(), vec![rows[0].clone()]);
        let other_text = fixture
            .app
            .world_mut()
            .run_system_once(move |mut queues: QueuedPrompts| queues.take_next(other))
            .unwrap()
            .unwrap()
            .1;
        assert_eq!(other_text, "other");
    }

    #[test]
    fn same_update_enqueue_initializes_once_and_retains_both_items() {
        let mut fixture = QueueFixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.session)
            .remove::<PromptQueue>();
        let session = fixture.session;
        fixture
            .app
            .world_mut()
            .run_system_once(move |mut queues: QueuedPrompts| {
                queues.enqueue(session, "first".into(), Vec::new(), false);
                queues.enqueue(session, "second".into(), Vec::new(), false);
            })
            .unwrap();
        assert_eq!(
            fixture.visible(),
            [(0, "first".into()), (1, "second".into())]
        );
        assert_eq!(fixture.take().unwrap().1, "first");
        assert_eq!(fixture.take().unwrap().1, "second");
    }

    #[test]
    fn same_update_cancel_and_dispatch_never_returns_the_canceled_item() {
        let mut fixture = QueueFixture::new();
        fixture.enqueue("cancel", Vec::new(), false);
        fixture.enqueue("send", Vec::new(), false);
        let canceled = fixture.visible()[0].0;
        let session = fixture.session;
        let dispatched = fixture
            .app
            .world_mut()
            .run_system_once(move |mut queues: QueuedPrompts| {
                assert!(queues.remove(session, canceled));
                let dispatched = queues.take_next(session).unwrap();
                assert!(queues.take_next(session).is_none());
                dispatched.1
            })
            .unwrap();
        assert_eq!(dispatched, "send");
        assert!(fixture.visible().is_empty());
    }

    #[test]
    fn pause_and_flush_cancellation_preserve_ordinary_fifo_dispatch() {
        let mut fixture = QueueFixture::new();
        fixture.enqueue("first", Vec::new(), false);
        fixture.enqueue("second", Vec::new(), false);
        fixture
            .app
            .world_mut()
            .get_mut::<PromptQueue>(fixture.session)
            .unwrap()
            .paused = true;
        assert!(fixture.take().is_none());
        assert!(fixture.flush());
        let session = fixture.session;
        fixture
            .app
            .world_mut()
            .run_system_once(move |mut queues: QueuedPrompts| queues.cancel_flush(session))
            .unwrap();
        assert_eq!(fixture.take().unwrap().1, "first");
        assert_eq!(fixture.take().unwrap().1, "second");
        assert!(fixture.take().is_none());
    }

    #[test]
    fn clearing_does_not_reuse_a_stale_cancel_id_and_order_compaction_preserves_fifo() {
        let mut fixture = QueueFixture::new();
        fixture.enqueue("old", Vec::new(), false);
        let old = fixture.visible()[0].0;
        let session = fixture.session;
        fixture
            .app
            .world_mut()
            .run_system_once(move |mut queues: QueuedPrompts| queues.clear(session))
            .unwrap();
        fixture.enqueue("first", Vec::new(), false);
        fixture
            .app
            .world_mut()
            .get_mut::<PromptQueue>(session)
            .unwrap()
            .next_order = u32::MAX;
        fixture.enqueue("second", Vec::new(), false);
        assert!(
            !fixture
                .app
                .world_mut()
                .run_system_once(move |mut queues: QueuedPrompts| queues.remove(session, old))
                .unwrap()
        );
        assert_eq!(fixture.take().unwrap().1, "first");
        assert_eq!(fixture.take().unwrap().1, "second");
    }

    #[test]
    fn a_foreign_queue_root_cannot_be_read_canceled_cleared_or_dispatched() {
        let mut fixture = QueueFixture::new();
        fixture.enqueue("private", Vec::new(), false);
        let id = fixture.visible()[0].0;
        let root = fixture
            .app
            .world()
            .get::<PromptQueue>(fixture.session)
            .unwrap()
            .root;
        let other = fixture
            .app
            .world_mut()
            .spawn((
                Session,
                PromptQueue {
                    root,
                    ..Default::default()
                },
            ))
            .id();
        let visible = fixture
            .app
            .world_mut()
            .run_system_once(move |rows: QueuedPromptReads| rows.visible(other).len())
            .unwrap();
        assert_eq!(visible, 0);
        fixture
            .app
            .world_mut()
            .run_system_once(move |mut queues: QueuedPrompts| {
                assert!(!queues.remove(other, id));
                queues.clear(other);
                assert!(queues.take_next(other).is_none());
            })
            .unwrap();
        assert_eq!(fixture.take().unwrap().1, "private");
    }
}
