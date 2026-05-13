use crate::terminal::Terminal;
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use std::collections::HashMap;
use vmux_core::PageMetadata;
use vmux_history::LastActivatedAt;
use vmux_terminal::event::TERMINAL_WEBVIEW_URL;

pub fn focus_pane_entity(entity: Entity, commands: &mut Commands, child_of_q: &Query<&ChildOf>) {
    commands.entity(entity).insert(LastActivatedAt::now());
    let mut current = entity;
    while let Ok(parent_rel) = child_of_q.get(current) {
        let parent = parent_rel.get();
        commands.entity(parent).insert(LastActivatedAt::now());
        current = parent;
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(pub u32);

#[derive(Resource, Default, Debug)]
pub struct PidToEntity(pub HashMap<u32, Entity>);

pub fn track_pid_inserts(
    mut map: ResMut<PidToEntity>,
    inserted: Query<(Entity, &Pid), Added<Pid>>,
) {
    for (entity, Pid(pid)) in &inserted {
        map.0.insert(*pid, entity);
    }
}

pub fn track_pid_removals(
    mut map: ResMut<PidToEntity>,
    mut removed: RemovedComponents<Pid>,
    survivors: Query<&Pid>,
) {
    for entity in removed.read() {
        if let Ok(Pid(pid)) = survivors.get(entity) {
            map.0.remove(pid);
        } else {
            map.0.retain(|_, &mut e| e != entity);
        }
    }
}

pub fn format_terminal_url(
    mut q: Query<
        (Option<&Pid>, &mut PageMetadata),
        (
            With<Terminal>,
            Without<crate::vibe::session::Vibe>,
            Or<(Changed<Pid>, Added<PageMetadata>)>,
        ),
    >,
) {
    for (pid, mut meta) in &mut q {
        let next = match pid {
            Some(Pid(p)) => format!("{TERMINAL_WEBVIEW_URL}{p}"),
            None => TERMINAL_WEBVIEW_URL.to_string(),
        };
        if meta.url != next {
            meta.url = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app() -> App {
        let mut app = App::new();
        app.init_resource::<PidToEntity>();
        app.add_systems(Update, (track_pid_inserts, track_pid_removals).chain());
        app
    }

    #[test]
    fn pid_insert_populates_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn(Pid(7777)).id();
        app.update();
        let map = app.world().resource::<PidToEntity>();
        assert_eq!(map.0.get(&7777), Some(&e));
    }

    #[test]
    fn entity_despawn_removes_pid_from_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn(Pid(8888)).id();
        app.update();
        app.world_mut().despawn(e);
        app.update();
        let map = app.world().resource::<PidToEntity>();
        assert!(!map.0.contains_key(&8888));
    }

    #[test]
    fn changing_pid_updates_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn(Pid(9000)).id();
        app.update();
        app.world_mut().entity_mut(e).remove::<Pid>();
        app.update();
        app.world_mut().entity_mut(e).insert(Pid(9001));
        app.update();
        let map = app.world().resource::<PidToEntity>();
        assert_eq!(map.0.get(&9001), Some(&e));
        assert!(!map.0.contains_key(&9000));
    }

    fn make_format_app() -> App {
        let mut app = App::new();
        app.add_systems(Update, format_terminal_url);
        app
    }

    fn empty_meta() -> PageMetadata {
        PageMetadata {
            title: String::new(),
            url: String::new(),
            favicon_url: String::new(),
            bg_color: None,
        }
    }

    #[test]
    fn formatter_emits_pid_url_for_terminal_with_pid() {
        let mut app = make_format_app();
        let e = app
            .world_mut()
            .spawn((Terminal, Pid(4242), empty_meta()))
            .id();
        app.update();
        let url = &app.world().get::<PageMetadata>(e).unwrap().url;
        assert_eq!(url, "vmux://terminal/4242");
    }

    #[test]
    fn formatter_emits_placeholder_for_terminal_without_pid() {
        let mut app = make_format_app();
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                PageMetadata {
                    url: "stale".into(),
                    ..empty_meta()
                },
            ))
            .id();
        app.update();
        let url = &app.world().get::<PageMetadata>(e).unwrap().url;
        assert_eq!(url, "vmux://terminal/");
    }
}
