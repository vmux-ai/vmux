//! Reload / fallback when a main-pane webview never delivers a real OSR frame (stuck on 1×1 placeholder).

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_layout::VmuxWebview;
use vmux_settings::VmuxAppSettings;

/// Wait long enough that slow pages can paint before we treat 1×1 OSR as a hard failure.
const FIRST_CHECK_DELAY_SECS: f32 = 6.0;
const RETRY_INTERVAL_SECS: f32 = 4.0;
const AFTER_NAVIGATE_SECS: f32 = 5.0;
const MAX_INITIAL_RELOADS: u32 = 3;
const MAX_POST_FALLBACK_RELOADS: u32 = 3;

const UNREACHABLE_HTML: &str = r#"<!DOCTYPE html><html><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width"/><style>html,body{margin:0;background:#1a1a1a;color:#bbb;font:14px system-ui,-apple-system,sans-serif;height:100%;}body{display:flex;align-items:center;justify-content:center;text-align:center;padding:1.5rem;}p{margin:.5rem 0;}small{opacity:.65;font-size:12px;}</style></head><body><div><p>Couldn’t load this page.</p><small>Network or renderer issue — try reloading (⌘R / Ctrl+R, or ⌘⇧R / Ctrl+Shift+R without cache) or another URL.</small></div></body></html>"#;

#[derive(Component, Debug)]
pub(crate) struct WebviewLoadWatchdog {
    pub next_deadline_secs: f32,
    pub initial_reloads: u32,
    pub did_navigate_fallback: bool,
    pub post_fallback_reloads: u32,
}

fn webview_texture_is_placeholder(
    images: &Assets<Image>,
    mat: &WebviewExtendStandardMaterial,
) -> bool {
    let Some(h) = mat.extension.surface.as_ref() else {
        return true;
    };
    images
        .get(h.id())
        .map(|img| img.width() <= 1 && img.height() <= 1)
        .unwrap_or(true)
}

pub(crate) fn add_webview_load_watchdog(
    mut commands: Commands,
    time: Res<Time>,
    q: Query<
        Entity,
        (
            With<VmuxWebview>,
            With<WebviewSource>,
            Without<WebviewLoadWatchdog>,
        ),
    >,
) {
    let now = time.elapsed_secs();
    for entity in &q {
        commands.entity(entity).insert(WebviewLoadWatchdog {
            next_deadline_secs: now + FIRST_CHECK_DELAY_SECS,
            initial_reloads: 0,
            did_navigate_fallback: false,
            post_fallback_reloads: 0,
        });
    }
}

pub(crate) fn webview_load_watchdog_tick(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<VmuxAppSettings>,
    images: Res<Assets<Image>>,
    materials: Res<Assets<WebviewExtendStandardMaterial>>,
    mut q: Query<
        (
            Entity,
            &WebviewSource,
            &MeshMaterial3d<WebviewExtendStandardMaterial>,
            &mut WebviewLoadWatchdog,
        ),
        With<VmuxWebview>,
    >,
) {
    let now = time.elapsed_secs();
    let fallback_url = settings.default_webview_url.trim();

    for (entity, source, mesh_mat, mut watchdog) in &mut q {
        let Some(mat) = materials.get(mesh_mat.id()) else {
            continue;
        };

        if !webview_texture_is_placeholder(&images, mat) {
            commands.entity(entity).remove::<WebviewLoadWatchdog>();
            continue;
        }

        if now < watchdog.next_deadline_secs {
            continue;
        }

        // 1) Reload a few times (original URL).
        if watchdog.initial_reloads < MAX_INITIAL_RELOADS {
            commands.trigger(RequestReload { webview: entity });
            watchdog.initial_reloads += 1;
            watchdog.next_deadline_secs = now + RETRY_INTERVAL_SECS;
            continue;
        }

        // 2) Navigate to configured default URL once (if different from current).
        if !watchdog.did_navigate_fallback {
            let already_fallback = match source {
                WebviewSource::Url(u) => fallback_url.is_empty() || u.trim() == fallback_url,
                WebviewSource::InlineHtml(_) => false,
            };

            if !already_fallback && !fallback_url.is_empty() {
                commands.trigger(RequestNavigate {
                    webview: entity,
                    url: fallback_url.to_string(),
                });
            }
            watchdog.did_navigate_fallback = true;
            watchdog.next_deadline_secs = now + AFTER_NAVIGATE_SECS;
            continue;
        }

        // 3) Reload a few times on the fallback page.
        if watchdog.post_fallback_reloads < MAX_POST_FALLBACK_RELOADS {
            commands.trigger(RequestReload { webview: entity });
            watchdog.post_fallback_reloads += 1;
            watchdog.next_deadline_secs = now + RETRY_INTERVAL_SECS;
            continue;
        }

        // 4) Guaranteed visible content: inline HTML (always renders without network).
        commands
            .entity(entity)
            .insert(WebviewSource::inline(UNREACHABLE_HTML));
        commands.entity(entity).remove::<WebviewLoadWatchdog>();
    }
}
