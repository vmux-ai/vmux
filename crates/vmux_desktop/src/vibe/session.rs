use bevy::prelude::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, mpsc};
use std::time::{Duration, SystemTime};

#[derive(Component, Debug)]
pub struct Vibe;

#[derive(Component, Debug, Clone)]
pub struct SessionId(pub String);

#[derive(Component, Debug)]
pub struct PendingVibeSession {
    pub spawn_time: SystemTime,
    pub cwd: PathBuf,
}

#[derive(Resource)]
pub struct VibeSessionWatcher {
    rx: Mutex<mpsc::Receiver<()>>,
    _watcher: RecommendedWatcher,
}

pub fn start_vibe_session_watcher(mut commands: Commands) {
    let root = vibe_sessions_root();
    if std::fs::create_dir_all(&root).is_err() {
        return;
    }
    let (tx, rx) = mpsc::channel();
    let watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
        if let Ok(event) = res
            && (event.kind.is_create() || event.kind.is_modify())
        {
            let _ = tx.send(());
        }
    });
    let Ok(mut watcher) = watcher else {
        bevy::log::warn!("vibe session watcher init failed");
        return;
    };
    if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
        bevy::log::warn!("vibe session watch({}) failed: {e}", root.display());
        return;
    }
    bevy::log::info!("watching {} for vibe session changes", root.display());
    commands.insert_resource(VibeSessionWatcher {
        rx: Mutex::new(rx),
        _watcher: watcher,
    });
}

fn drain_watcher(watcher: &VibeSessionWatcher) -> bool {
    let Ok(rx) = watcher.rx.lock() else {
        return false;
    };
    let mut had = false;
    while rx.try_recv().is_ok() {
        had = true;
    }
    had
}

#[derive(Resource, Default)]
pub struct VibeSessionDirty(pub bool);

pub fn mark_vibe_session_dirty_on_change(
    watcher: Option<Res<VibeSessionWatcher>>,
    mut dirty: ResMut<VibeSessionDirty>,
) {
    let Some(watcher) = watcher else { return };
    if drain_watcher(&watcher) {
        dirty.0 = true;
    }
}

pub fn mark_vibe_session_dirty_on_pending_added(
    added_pending: Query<(), Added<PendingVibeSession>>,
    added_session: Query<(), Added<SessionId>>,
    mut dirty: ResMut<VibeSessionDirty>,
) {
    if !added_pending.is_empty() || !added_session.is_empty() {
        dirty.0 = true;
    }
}

pub fn vibe_session_dirty_run_condition(dirty: Res<VibeSessionDirty>) -> bool {
    dirty.0
}

pub fn clear_vibe_session_dirty(mut dirty: ResMut<VibeSessionDirty>) {
    dirty.0 = false;
}

pub const PENDING_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Resource, Default, Debug)]
pub struct VibeSessionToEntity(pub HashMap<String, Entity>);

pub fn track_session_id_inserts(
    mut map: ResMut<VibeSessionToEntity>,
    inserted: Query<(Entity, &SessionId), (Added<SessionId>, With<Vibe>)>,
) {
    for (entity, SessionId(id)) in &inserted {
        map.0.insert(id.clone(), entity);
    }
}

pub fn track_session_id_removals(
    mut map: ResMut<VibeSessionToEntity>,
    mut removed: RemovedComponents<SessionId>,
) {
    for entity in removed.read() {
        map.0.retain(|_, &mut e| e != entity);
    }
}

#[derive(serde::Deserialize)]
struct MetaEnvironment {
    working_directory: String,
}

#[derive(serde::Deserialize)]
struct MetaJson {
    session_id: String,
    start_time: String,
    environment: MetaEnvironment,
}

fn normalize_cwd(path: &std::path::Path) -> String {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canon.to_string_lossy().trim_end_matches('/').to_string()
}

pub(crate) fn discover_session_id_for(
    sessions_root: &std::path::Path,
    cwd: &std::path::Path,
    spawn_time: SystemTime,
    claimed: &std::collections::HashSet<String>,
) -> Option<String> {
    let cwd_norm = normalize_cwd(cwd);
    let spawn_secs = spawn_time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let entries = std::fs::read_dir(sessions_root).ok()?;
    let mut best: Option<(i64, String)> = None;

    for entry in entries.flatten() {
        let meta_path = entry.path().join("meta.json");
        let Ok(text) = std::fs::read_to_string(&meta_path) else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<MetaJson>(&text) else {
            continue;
        };
        let meta_cwd = normalize_cwd(std::path::Path::new(&meta.environment.working_directory));
        if meta_cwd != cwd_norm {
            continue;
        }
        if claimed.contains(&meta.session_id) {
            continue;
        }
        let Ok(start_dt) = chrono::DateTime::parse_from_rfc3339(&meta.start_time) else {
            continue;
        };
        let start_secs = start_dt.timestamp();
        if start_secs < spawn_secs {
            continue;
        }
        match &best {
            None => best = Some((start_secs, meta.session_id)),
            Some((cur, _)) if start_secs < *cur => best = Some((start_secs, meta.session_id)),
            _ => {}
        }
    }

    best.map(|(_, id)| id)
}

pub fn vibe_sessions_root() -> PathBuf {
    std::env::var("VIBE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join(".vibe")
        })
        .join("logs")
        .join("session")
}

pub const VIBE_WEBVIEW_URL: &str = "vmux://vibe/";

pub fn format_vibe_url(
    mut q: Query<
        (Option<&SessionId>, &mut vmux_core::PageMetadata),
        (
            With<Vibe>,
            Or<(
                Changed<SessionId>,
                Added<vmux_core::PageMetadata>,
                Added<Vibe>,
            )>,
        ),
    >,
) {
    for (sid, mut meta) in &mut q {
        let next = match sid {
            Some(SessionId(id)) => format!("{VIBE_WEBVIEW_URL}{id}"),
            None => VIBE_WEBVIEW_URL.to_string(),
        };
        if meta.url != next {
            meta.url = next;
        }
    }
}

pub fn detect_vibe_session_exit_on_change(
    mut commands: Commands,
    mut q: Query<(Entity, &SessionId, &mut vmux_core::PageMetadata), With<Vibe>>,
    pid_q: Query<&crate::terminal::pid::Pid>,
) {
    let sessions_root = vibe_sessions_root();
    let Ok(entries) = std::fs::read_dir(&sessions_root) else {
        return;
    };
    let mut session_dirs: HashMap<String, std::path::PathBuf> = HashMap::new();
    for entry in entries.flatten() {
        let meta_path = entry.path().join("meta.json");
        let Ok(text) = std::fs::read_to_string(&meta_path) else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<MetaJsonHead>(&text) else {
            continue;
        };
        session_dirs.insert(meta.session_id, meta_path);
    }
    for (entity, SessionId(id), mut meta) in &mut q {
        let Some(meta_path) = session_dirs.get(id) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(meta_path) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<MetaJsonExit>(&text) else {
            continue;
        };
        if parsed.end_time.is_some() {
            bevy::log::info!("vibe session {id} ended; reverting entity {entity:?} to terminal");
            commands
                .entity(entity)
                .remove::<Vibe>()
                .remove::<SessionId>();
            let next = match pid_q.get(entity).ok() {
                Some(crate::terminal::pid::Pid(p)) => {
                    format!("{}{}", vmux_terminal::event::TERMINAL_WEBVIEW_URL, p)
                }
                None => vmux_terminal::event::TERMINAL_WEBVIEW_URL.to_string(),
            };
            if meta.url != next {
                meta.url = next;
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct MetaJsonHead {
    session_id: String,
}

#[derive(serde::Deserialize)]
struct MetaJsonExit {
    end_time: Option<String>,
}

pub fn discover_pending_vibe_sessions_on_change(
    mut commands: Commands,
    mut q: Query<(Entity, &PendingVibeSession), With<Vibe>>,
    map: Res<VibeSessionToEntity>,
) {
    let sessions_root = vibe_sessions_root();
    let claimed: std::collections::HashSet<String> = map.0.keys().cloned().collect();
    let now = SystemTime::now();
    for (entity, pending) in &mut q {
        if let Some(id) =
            discover_session_id_for(&sessions_root, &pending.cwd, pending.spawn_time, &claimed)
        {
            bevy::log::info!(
                "vibe session discovered for entity {entity:?}: {id} (cwd={})",
                pending.cwd.display()
            );
            commands
                .entity(entity)
                .insert(SessionId(id))
                .remove::<PendingVibeSession>();
            continue;
        }
        if now.duration_since(pending.spawn_time).unwrap_or_default() >= PENDING_DISCOVERY_TIMEOUT {
            bevy::log::warn!(
                "vibe session discovery timed out for entity {entity:?} (cwd={}, sessions_root={})",
                pending.cwd.display(),
                sessions_root.display()
            );
            commands.entity(entity).remove::<PendingVibeSession>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app() -> App {
        let mut app = App::new();
        app.init_resource::<VibeSessionToEntity>();
        app.add_systems(
            Update,
            (track_session_id_inserts, track_session_id_removals).chain(),
        );
        app
    }

    #[test]
    fn session_insert_populates_map_only_for_vibe_entities() {
        let mut app = make_app();
        let with_vibe = app.world_mut().spawn((Vibe, SessionId("abc".into()))).id();
        let without_vibe = app.world_mut().spawn(SessionId("xyz".into())).id();
        app.update();
        let map = app.world().resource::<VibeSessionToEntity>();
        assert_eq!(map.0.get("abc"), Some(&with_vibe));
        assert!(!map.0.contains_key("xyz"));
        let _ = without_vibe;
    }

    #[test]
    fn entity_despawn_removes_session_from_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn((Vibe, SessionId("def".into()))).id();
        app.update();
        app.world_mut().despawn(e);
        app.update();
        let map = app.world().resource::<VibeSessionToEntity>();
        assert!(!map.0.contains_key("def"));
    }

    fn write_meta(dir: &std::path::Path, session_id: &str, working_dir: &str, start_time: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("meta.json"),
            format!(
                r#"{{"session_id":"{session_id}","start_time":"{start_time}","environment":{{"working_directory":"{working_dir}"}}}}"#
            ),
        )
        .unwrap();
    }

    fn unique_tmp(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("vmux-test-{label}-{pid}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn discover_picks_session_matching_cwd_and_after_spawn_time() {
        let tmp = unique_tmp("vibe-discover");
        let sessions = tmp.join("sessions");
        let cwd = "/tmp/work-A";

        write_meta(
            &sessions.join("session_20260101_080000_olderold"),
            "older-uuid",
            cwd,
            "2025-12-31T23:00:00+00:00",
        );
        write_meta(
            &sessions.join("session_20260511_120000_thisone"),
            "this-uuid",
            cwd,
            "2026-05-11T12:00:00+00:00",
        );
        write_meta(
            &sessions.join("session_20260511_120000_other"),
            "other-uuid",
            "/tmp/work-B",
            "2026-05-11T12:00:00+00:00",
        );

        let spawn_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_770_000_000);
        let claimed: std::collections::HashSet<String> = std::collections::HashSet::new();
        let result =
            discover_session_id_for(&sessions, std::path::Path::new(cwd), spawn_time, &claimed);
        assert_eq!(result.as_deref(), Some("this-uuid"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn empty_meta() -> vmux_core::PageMetadata {
        vmux_core::PageMetadata {
            title: String::new(),
            url: String::new(),
            favicon_url: String::new(),
            bg_color: None,
        }
    }

    #[test]
    fn formatter_emits_session_url_for_vibe_with_session() {
        let mut app = App::new();
        app.add_systems(Update, format_vibe_url);
        let e = app
            .world_mut()
            .spawn((
                Vibe,
                SessionId("ae724a54-c387-5359-0687-ccfc155558b6".into()),
                empty_meta(),
            ))
            .id();
        app.update();
        let url = &app.world().get::<vmux_core::PageMetadata>(e).unwrap().url;
        assert_eq!(url, "vmux://vibe/ae724a54-c387-5359-0687-ccfc155558b6");
    }

    #[test]
    fn formatter_emits_placeholder_for_vibe_without_session() {
        let mut app = App::new();
        app.add_systems(Update, format_vibe_url);
        let e = app
            .world_mut()
            .spawn((
                Vibe,
                vmux_core::PageMetadata {
                    url: "stale".into(),
                    ..empty_meta()
                },
            ))
            .id();
        app.update();
        let url = &app.world().get::<vmux_core::PageMetadata>(e).unwrap().url;
        assert_eq!(url, "vmux://vibe/");
    }

    #[test]
    fn discover_skips_already_claimed_sessions() {
        let tmp = unique_tmp("vibe-claimed");
        let sessions = tmp.join("sessions");
        let cwd = "/tmp/work";

        write_meta(
            &sessions.join("session_a"),
            "claimed-uuid",
            cwd,
            "2026-05-11T12:00:00+00:00",
        );
        write_meta(
            &sessions.join("session_b"),
            "free-uuid",
            cwd,
            "2026-05-11T12:00:01+00:00",
        );

        let spawn_time = SystemTime::UNIX_EPOCH;
        let mut claimed = std::collections::HashSet::new();
        claimed.insert("claimed-uuid".to_string());

        let result =
            discover_session_id_for(&sessions, std::path::Path::new(cwd), spawn_time, &claimed);
        assert_eq!(result.as_deref(), Some("free-uuid"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
