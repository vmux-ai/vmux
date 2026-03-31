//! Session persistence with [moonshine-save]: hierarchical layout snapshot + legacy URL resource.

use std::path::PathBuf;

use bevy::app::AppExit;
use bevy::prelude::*;
use ron::value::{Map, Value};
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use vmux_core::{NavigationHistory, NavigationHistoryFile, WebviewDocumentUrlEmit};
pub use vmux_core::{
    NavigationHistoryPath, NavigationHistorySaveQueue, SessionSavePath, SessionSaveQueue,
};
use vmux_layout::{
    History, LayoutTree, PaneLastUrl, Root, SessionLayoutSnapshot, WebviewPane,
    allowed_navigation_url,
};
use vmux_settings::{
    VmuxAppSettings, VmuxCacheDir, VmuxCacheDirInitSet, resolve_window_padding_top_px_for_load,
};
use vmux_webview::{rebuild_session_snapshot, setup_vmux_panes_startup};

const SAVE_FILENAME: &str = "last_session.ron";
const NAV_HISTORY_FILENAME: &str = "navigation_history.ron";

fn session_save_path(cache: &VmuxCacheDir) -> PathBuf {
    cache
        .0
        .clone()
        .map(|d| d.join(SAVE_FILENAME))
        .unwrap_or_else(|| std::env::temp_dir().join("vmux_last_session.ron"))
}

fn navigation_history_path(cache: &VmuxCacheDir) -> PathBuf {
    cache
        .0
        .clone()
        .map(|d| d.join(NAV_HISTORY_FILENAME))
        .unwrap_or_else(|| std::env::temp_dir().join("vmux_navigation_history.ron"))
}

fn init_session_save_path(mut commands: Commands, cache: Res<VmuxCacheDir>) {
    commands.insert_resource(SessionSavePath(session_save_path(&cache)));
    commands.insert_resource(NavigationHistoryPath(navigation_history_path(&cache)));
}

fn load_navigation_history_from_disk(mut commands: Commands, path: Res<NavigationHistoryPath>) {
    let p = &path.0;
    let hist = if p.is_file() {
        match std::fs::read_to_string(p) {
            Ok(s) => match ron::from_str::<NavigationHistoryFile>(&s) {
                Ok(f) => NavigationHistory::from(f),
                Err(e) => {
                    warn!("vmux_session: bad navigation_history.ron {:?}: {e}", p);
                    NavigationHistory::default()
                }
            },
            Err(e) => {
                warn!("vmux_session: read navigation_history.ron {:?}: {e}", p);
                NavigationHistory::default()
            }
        }
    } else {
        NavigationHistory::default()
    };
    commands.insert_resource(hist);
}

/// Moonshine deserializes [`VmuxAppSettings`] via Reflect (not serde), so old session files still
/// use `pane_gap_px` / `window_edge_gap_px`. Rewrite those keys before load so upgrades do not panic.
fn migrate_last_session_vmux_settings_keys(path: Res<SessionSavePath>) {
    let path = &path.0;
    if !path.is_file() {
        return;
    }
    let Ok(mut s) = std::fs::read_to_string(path) else {
        return;
    };
    let before = s.clone();
    s = s.replace("pane_gap_px:", "pane_border_spacing_px:");
    s = s.replace("window_edge_gap_px:", "window_padding_px:");
    if s != before {
        if let Err(e) = std::fs::write(path, s.as_bytes()) {
            warn!(
                "vmux_session: could not migrate session file {:?}: {e}",
                path
            );
        }
    }
}

struct FlatSessionVmuxSettings {
    default_webview_url: String,
    pane_border_spacing_px: f32,
    window_padding_px: f32,
    window_padding_top_px: f32,
    pane_border_radius_px: f32,
}

fn default_layout_f32() -> f32 {
    8.0
}

fn ron_map_key_str(k: &Value) -> Option<&str> {
    match k {
        Value::String(s) => Some(s.as_str()),
        _ => None,
    }
}

fn value_as_f32(v: &Value) -> Option<f32> {
    match v {
        Value::Number(n) => Some(n.into_f64() as f32),
        _ => None,
    }
}

fn get_ron_map_field<'a>(map: &'a Map, names: &[&str]) -> Option<&'a Value> {
    for (k, v) in map.iter() {
        let key = ron_map_key_str(k)?;
        if names.contains(&key) {
            return Some(v);
        }
    }
    None
}

/// Parse flat `VmuxAppSettings` field list (inside `…( … )`) ignoring unknown keys (old saves, extra reflect fields).
fn flat_from_ron_struct_body(body: &str) -> Option<FlatSessionVmuxSettings> {
    let inner = body.trim();
    // Bevy reflect uses RON struct tuples `( key: value, )`; `ron::value::Map` expects `{ … }` syntax.
    let wrapped = format!("({inner})");
    let root: Value = ron::from_str(&wrapped).ok()?;
    let map = match root {
        Value::Map(m) => m,
        _ => return None,
    };
    let default_webview_url = get_ron_map_field(&map, &["default_webview_url"])
        .and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })?;
    let pane_border_spacing_px = get_ron_map_field(&map, &["pane_border_spacing_px", "pane_gap_px"])
        .and_then(value_as_f32)
        .unwrap_or_else(default_layout_f32);
    let window_padding_px = get_ron_map_field(&map, &["window_padding_px", "window_edge_gap_px"])
        .and_then(value_as_f32)
        .unwrap_or_else(default_layout_f32);
    let window_padding_top_px = get_ron_map_field(&map, &["window_padding_top_px"])
        .and_then(value_as_f32)
        .unwrap_or(0.0);
    let pane_border_radius_px = get_ron_map_field(&map, &["pane_border_radius_px"])
        .and_then(value_as_f32)
        .unwrap_or_else(default_layout_f32);
    Some(FlatSessionVmuxSettings {
        default_webview_url,
        pane_border_spacing_px,
        window_padding_px,
        window_padding_top_px,
        pane_border_radius_px,
    })
}

fn nested_vmux_app_settings_ron(flat: &FlatSessionVmuxSettings) -> Option<String> {
    let top = resolve_window_padding_top_px_for_load(
        flat.window_padding_px,
        flat.window_padding_top_px,
    );
    let url_ron = ron::to_string(&flat.default_webview_url).ok()?;
    let sp = ron::to_string(&flat.pane_border_spacing_px).ok()?;
    let wp = ron::to_string(&flat.window_padding_px).ok()?;
    let wt = ron::to_string(&top).ok()?;
    let br = ron::to_string(&flat.pane_border_radius_px).ok()?;
    Some(format!(
        "VmuxAppSettings(\n        browser: VmuxBrowserSettings(\n            default_webview_url: {url_ron},\n        ),\n        layout: VmuxLayoutSettings(\n            pane_border_spacing_px: {sp},\n            window_padding_px: {wp},\n            window_padding_top_px: {wt},\n            pane_border_radius_px: {br},\n        ),\n    )"
    ))
}

fn skip_ron_string(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i = (i + 2).min(bytes.len()),
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

fn matching_close_paren(content: &str, open_paren: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    if bytes.get(open_paren) != Some(&b'(') {
        return None;
    }
    let mut depth = 0u32;
    let mut i = open_paren;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                i += 1;
            }
            b'"' => {
                i = skip_ron_string(bytes, i + 1);
            }
            _ => i += 1,
        }
    }
    None
}

/// RON map key for [`VmuxAppSettings`] in Bevy dynamic scenes (see `bevy_scene` `SceneMapSerializer`).
const VMUX_APP_SETTINGS_QUOTED_TYPE_KEY: &str = "\"vmux_settings::VmuxAppSettings\"";

/// Flat [`VmuxAppSettings`] in moonshine RON cannot deserialize after nested `browser` / `layout`;
/// rewrite before [`LoadWorld`].
///
/// Bevy may serialize the resource value as bare `( fields )` without a `VmuxAppSettings` prefix
/// (RON `struct_names` off), so we migrate using the `"vmux_settings::VmuxAppSettings"` map entry first.
fn migrate_flat_vmux_app_settings_in_session_ron(content: &mut String) -> bool {
    let a = migrate_vmux_app_settings_after_type_key(content);
    let b = migrate_vmux_app_settings_open_marker(content);
    a || b
}

fn migrate_vmux_app_settings_after_type_key(content: &mut String) -> bool {
    let mut any = false;
    let mut search_from = 0usize;
    while let Some(rel) = content
        .get(search_from..)
        .and_then(|h| h.find(VMUX_APP_SETTINGS_QUOTED_TYPE_KEY))
    {
        let key_pos = search_from + rel;
        let after_key = key_pos + VMUX_APP_SETTINGS_QUOTED_TYPE_KEY.len();
        let Some(tail) = content.get(after_key..) else {
            break;
        };
        let mut i = after_key + (tail.len() - tail.trim_start().len());
        let bytes = content.as_bytes();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) != Some(&b':') {
            search_from = key_pos + 1;
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let replace_start;
        if content.get(i..).is_some_and(|s| s.starts_with("VmuxAppSettings")) {
            replace_start = i;
            i += "VmuxAppSettings".len();
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
        } else {
            replace_start = i;
        }
        if bytes.get(i) != Some(&b'(') {
            search_from = key_pos + 1;
            continue;
        }
        let open_paren = i;
        let Some(close_paren) = matching_close_paren(content, open_paren) else {
            search_from = key_pos + 1;
            continue;
        };
        let body = &content[open_paren + 1..close_paren];
        if body.trim_start().starts_with("browser:") {
            search_from = close_paren + 1;
            continue;
        }
        let Some(flat) = flat_from_ron_struct_body(body) else {
            search_from = key_pos + 1;
            continue;
        };
        let Some(replacement) = nested_vmux_app_settings_ron(&flat) else {
            search_from = key_pos + 1;
            continue;
        };
        let old_end = close_paren + 1;
        content.replace_range(replace_start..old_end, &replacement);
        any = true;
        search_from = replace_start + replacement.len();
    }
    any
}

fn migrate_vmux_app_settings_open_marker(content: &mut String) -> bool {
    const MARKER: &str = "VmuxAppSettings";
    let mut any = false;
    let mut search_from = 0usize;
    while let Some(rel) = content.get(search_from..).and_then(|h| h.find(MARKER)) {
        let abs = search_from + rel;
        // Skip `::VmuxAppSettings` inside the quoted reflect type path `vmux_settings::VmuxAppSettings`.
        if abs >= 2 && content.get(abs - 2..abs) == Some("::") {
            search_from = abs + 1;
            continue;
        }
        let after_marker = abs + MARKER.len();
        let Some(tail) = content.get(after_marker..) else {
            break;
        };
        let ws = tail.len() - tail.trim_start().len();
        let open_paren = after_marker + ws;
        if content.as_bytes().get(open_paren) != Some(&b'(') {
            search_from = abs + 1;
            continue;
        }
        let Some(close_paren) = matching_close_paren(content, open_paren) else {
            search_from = abs + 1;
            continue;
        };
        let body = &content[open_paren + 1..close_paren];
        if body.trim_start().starts_with("browser:") {
            search_from = close_paren + 1;
            continue;
        }
        let Some(flat) = flat_from_ron_struct_body(body) else {
            search_from = abs + 1;
            continue;
        };
        let Some(replacement) = nested_vmux_app_settings_ron(&flat) else {
            search_from = abs + 1;
            continue;
        };
        let old_end = close_paren + 1;
        content.replace_range(abs..old_end, &replacement);
        any = true;
        search_from = abs + replacement.len();
    }
    any
}

fn migrate_last_session_vmux_app_settings_nested(path: Res<SessionSavePath>) {
    let path = &path.0;
    if !path.is_file() {
        return;
    }
    let Ok(mut s) = std::fs::read_to_string(path) else {
        return;
    };
    if !migrate_flat_vmux_app_settings_in_session_ron(&mut s) {
        return;
    }
    if let Err(e) = std::fs::write(path, s.as_bytes()) {
        warn!(
            "vmux_session: could not migrate nested VmuxAppSettings in {:?}: {e}",
            path
        );
    } else {
        info!(
            "vmux_session: migrated flat `VmuxAppSettings` in {:?} to nested `browser` / `layout`",
            path
        );
    }
}

fn load_session(mut commands: Commands, path: PathBuf) {
    if path.is_file() {
        commands.trigger_load(LoadWorld::default_from_file(path));
    }
}

fn load_session_from_resource(commands: Commands, path: Res<SessionSavePath>) {
    load_session(commands, path.0.clone());
}

/// Writes [`SessionLayoutSnapshot`] and [`VmuxAppSettings`] to `path` (same file moonshine loads in `vmux`).
pub fn save_session_snapshot_to_file(commands: &mut Commands, path: PathBuf) {
    commands.trigger_save(
        SaveWorld::default_into_file(path)
            .include_resource::<SessionLayoutSnapshot>()
            .include_resource::<VmuxAppSettings>(),
    );
}

pub fn save_navigation_history_to_disk(path: &PathBuf, hist: &NavigationHistory) {
    let file = NavigationHistoryFile::from(hist);
    let Ok(s) = ron::to_string(&file) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(path, s.as_bytes()) {
        warn!("vmux_session: write navigation_history {:?}: {e}", path);
    }
}

fn drain_session_save_queue_inner(queue: &mut SessionSaveQueue, commands: &mut Commands) {
    let paths = std::mem::take(&mut queue.0);
    for path in paths {
        save_session_snapshot_to_file(commands, path);
    }
}

fn drain_session_save_queue_startup(mut queue: ResMut<SessionSaveQueue>, mut commands: Commands) {
    drain_session_save_queue_inner(&mut queue, &mut commands);
}

fn drain_session_save_queue_update(mut queue: ResMut<SessionSaveQueue>, mut commands: Commands) {
    drain_session_save_queue_inner(&mut queue, &mut commands);
}

fn drain_navigation_history_save_queue(
    mut queue: ResMut<NavigationHistorySaveQueue>,
    path: Res<NavigationHistoryPath>,
    hist: Res<NavigationHistory>,
) {
    let paths = std::mem::take(&mut queue.0);
    if paths.is_empty() {
        return;
    }
    save_navigation_history_to_disk(&path.0, &hist);
}

fn on_webview_document_url(
    trigger: On<Receive<WebviewDocumentUrlEmit>>,
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    mut pane_queries: ParamSet<(Query<&mut PaneLastUrl>, Query<&PaneLastUrl>)>,
    layout_q: Query<&LayoutTree, With<Root>>,
    webview_src: Query<&WebviewSource>,
    history_overlay: Query<Entity, (With<WebviewPane>, With<History>)>,
    (path, settings): (Res<SessionSavePath>, Res<VmuxAppSettings>),
    mut session_queue: ResMut<SessionSaveQueue>,
    mut nav_hist: ResMut<NavigationHistory>,
    nav_path: Res<NavigationHistoryPath>,
    mut nav_queue: ResMut<NavigationHistorySaveQueue>,
) {
    let ev = trigger.event();
    let webview = ev.webview;
    if history_overlay.contains(webview) {
        return;
    }
    let Some(url) = ev.url.as_deref() else {
        return;
    };
    let url = url.trim();
    if url.is_empty() || !allowed_navigation_url(url) {
        return;
    }
    // `PaneLastUrl` is seeded with the spawn URL; the preload script's first `{ url }` emit matches it.
    // We must still record that visit in `NavigationHistory` (deduped inside `push_visit`).
    let url_changed = {
        let mut q = pane_queries.p0();
        let Ok(mut pl) = q.get_mut(webview) else {
            return;
        };
        if pl.0.as_str() != url {
            pl.0 = url.to_string();
            true
        } else {
            false
        }
    };
    if nav_hist.push_visit(url.to_string()) {
        nav_queue.0.push(nav_path.0.clone());
    }
    if url_changed {
        let Ok(tree) = layout_q.single() else {
            return;
        };
        *snapshot = rebuild_session_snapshot(
            tree,
            &pane_queries.p1(),
            &webview_src,
            &history_overlay,
            settings.browser.default_webview_url.as_str(),
        );
        session_queue.0.push(path.0.clone());
    }
}

/// Flushes session to disk on shutdown. `AppExit` uses Bevy’s message bus, not the ECS observer
/// `Event` system, so `add_observer(On<AppExit>)` is not applicable; `MessageReader` in `Last` is correct.
fn save_session_on_app_exit(
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    layout_q: Query<&LayoutTree, With<Root>>,
    pane_last: Query<&PaneLastUrl>,
    webview_src: Query<&WebviewSource>,
    history_panes: Query<Entity, (With<WebviewPane>, With<History>)>,
    settings: Res<VmuxAppSettings>,
    path: Res<SessionSavePath>,
    nav_path: Res<NavigationHistoryPath>,
    hist: Res<NavigationHistory>,
    mut exits: MessageReader<AppExit>,
    mut commands: Commands,
) {
    for _ in exits.read() {
        match layout_q.single() {
            Ok(tree) => {
                *snapshot = rebuild_session_snapshot(
                    tree,
                    &pane_last,
                    &webview_src,
                    &history_panes,
                    settings.browser.default_webview_url.as_str(),
                );
            }
            Err(e) => {
                warn!("vmux_session: skip layout snapshot rebuild on exit (no Root / LayoutTree?): {e}");
            }
        }
        save_session_snapshot_to_file(&mut commands, path.0.clone());
        save_navigation_history_to_disk(&nav_path.0, &hist);
    }
}

/// Registers layout snapshot, legacy URL resource, session path, moonshine load/save, and URL observer.
#[derive(Default)]
pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SessionLayoutSnapshot>()
            .init_resource::<SessionSaveQueue>()
            .init_resource::<NavigationHistorySaveQueue>()
            .add_observer(moonshine_save::prelude::save_on_default_event)
            .add_observer(moonshine_save::prelude::load_on_default_event)
            .add_observer(on_webview_document_url)
            .add_systems(
                PreStartup,
                (
                    init_session_save_path.after(VmuxCacheDirInitSet),
                    migrate_last_session_vmux_settings_keys.after(init_session_save_path),
                    migrate_last_session_vmux_app_settings_nested
                        .after(migrate_last_session_vmux_settings_keys),
                    load_navigation_history_from_disk.after(init_session_save_path),
                    load_session_from_resource.after(migrate_last_session_vmux_app_settings_nested),
                ),
            )
            .add_systems(
                Startup,
                drain_session_save_queue_startup.after(setup_vmux_panes_startup),
            )
            // After all `Update` systems and observers (including URL `Receive`), so enqueued paths flush same frame.
            .add_systems(
                PostUpdate,
                (
                    drain_session_save_queue_update,
                    drain_navigation_history_save_queue.after(drain_session_save_queue_update),
                ),
            )
            .add_systems(Last, save_session_on_app_exit);
    }
}

#[cfg(test)]
mod ecs_tests {
    use super::*;

    #[test]
    fn session_plugin_registers_in_app() {
        let mut app = App::new();
        app.add_plugins(SessionPlugin);
    }
}

#[cfg(test)]
#[test]
fn flat_from_ron_struct_body_parses_unquoted_keys() {
    let body = r#"default_webview_url: "https://a.com", pane_border_spacing_px: 8.0, window_padding_px: 8.0, window_padding_top_px: 28.0, pane_border_radius_px: 8.0"#;
    let f = flat_from_ron_struct_body(body).expect("map parse");
    assert_eq!(f.default_webview_url, "https://a.com");
    assert!((f.pane_border_spacing_px - 8.0).abs() < f32::EPSILON);
}

#[cfg(test)]
#[test]
fn migrate_bare_tuple_after_type_key() {
    let mut s = concat!(
        r#"resources: { "#,
        r#""vmux_settings::VmuxAppSettings":"#,
        " ( default_webview_url: \"https://z.com\", pane_border_spacing_px: 8.0, ",
        "window_padding_px: 8.0, window_padding_top_px: 28.0, pane_border_radius_px: 8.0, ), ",
        "},\n",
    )
    .to_string();
    assert!(migrate_vmux_app_settings_after_type_key(&mut s));
    assert!(s.contains("browser:") && s.contains("VmuxBrowserSettings"));
}
