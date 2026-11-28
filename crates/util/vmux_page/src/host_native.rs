use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use bevy_ecs::prelude::Entity;
use bevy_winit::{EventLoopProxy, WINIT_WINDOWS, WinitUserEvent};
use crossbeam_channel::{Receiver, Sender};
use tracing::error;
use vmux_ecs::UiReceiveRaw;
use vmux_ui::hooks::EventListenerError;

use crate::host::{PageAppearance, PageBounds};
use crate::{
    Appearance, AssetReply, Assets as RendererAssets, Embedding, Instance, LifecycleSink,
    LifecycleUpdate, Outbox as RendererOutbox, Page, Wake as RendererWake, WebView,
};

pub(crate) struct NativeEvent {
    pub(crate) page: Entity,
    pub(crate) update: NativeUpdate,
}

pub(crate) enum NativeUpdate {
    UiEvent(UiReceiveRaw),
    Lifecycle(LifecycleUpdate),
}

struct MountedWebView {
    webview: WebView,
    window: Entity,
    bounds: PageBounds,
    appearance: PageAppearance,
    generation: u64,
}

pub(crate) struct NativeRenderer {
    pages: HashMap<Entity, MountedWebView>,
    sender: Sender<NativeEvent>,
    receiver: Receiver<NativeEvent>,
}

impl Default for NativeRenderer {
    fn default() -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        Self {
            pages: HashMap::new(),
            sender,
            receiver,
        }
    }
}

impl NativeRenderer {
    pub(crate) fn drain(&self) -> Vec<NativeEvent> {
        let mut drained = Vec::new();
        while let Ok(event) = self.receiver.try_recv() {
            drained.push(event);
        }
        drained
    }

    pub(crate) fn contains(&self, entity: Entity) -> bool {
        self.pages.contains_key(&entity)
    }

    pub(crate) fn mounted_entities(&self) -> Vec<Entity> {
        self.pages.keys().copied().collect()
    }

    pub(crate) fn window_of(&self, entity: Entity) -> Option<Entity> {
        self.pages.get(&entity).map(|mounted| mounted.window)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mount(
        &mut self,
        entity: Entity,
        window_entity: Entity,
        page: &'static Page,
        bounds: PageBounds,
        appearance: PageAppearance,
        interactive: bool,
        generation: u64,
        stylesheet: Option<String>,
        asset_directory: Option<PathBuf>,
        proxy: Option<EventLoopProxy<WinitUserEvent>>,
    ) -> Result<(u64, u64), String> {
        let page_url = Arc::new(Mutex::new(page.url.to_string()));
        let navigation_revision = Rc::new(Cell::new(1));
        let ipc_epoch = Rc::new(Cell::new(1));
        let waker: Rc<dyn RendererWake> = Rc::new(WinitPageWake { proxy });
        let embed = Embedding {
            outbox: Rc::new(ChannelOutbox {
                page: entity,
                generation,
                page_url,
                sender: self.sender.clone(),
                waker: Rc::clone(&waker),
            }),
            assets: Rc::new(StaticAssets {
                default_css: stylesheet,
                directory: asset_directory,
            }),
            waker: Rc::clone(&waker),
            lifecycle: Rc::new(ChannelLifecycle {
                page: entity,
                sender: self.sender.clone(),
                waker,
            }),
            navigation_revision,
            ipc_epoch,
        };
        let rect = to_wry_rect(bounds);
        let native_appearance = to_webview_appearance(appearance);
        let webview = WINIT_WINDOWS.with_borrow(|windows| {
            let Some(window) = windows.get_window(window_entity) else {
                return Err("target winit window is not ready".to_string());
            };
            WebView::build(
                page,
                &**window,
                rect,
                embed,
                Instance::default(),
                native_appearance,
                interactive,
            )
            .map_err(|error| error.to_string())
        })?;
        webview.set_visible(true);
        let revision = webview.navigation_revision().max(1);
        let epoch = webview.ipc_epoch().max(1);
        self.pages.insert(
            entity,
            MountedWebView {
                webview,
                window: window_entity,
                bounds,
                appearance,
                generation,
            },
        );
        Ok((revision, epoch))
    }

    pub(crate) fn unmount(&mut self, entity: Entity) -> bool {
        if let Some(mounted) = self.pages.remove(&entity) {
            mounted.webview.set_visible(false);
            return true;
        }
        false
    }

    pub(crate) fn set_bounds(&mut self, entity: Entity, bounds: PageBounds) {
        let Some(mounted) = self.pages.get_mut(&entity) else {
            return;
        };
        if mounted.bounds == bounds {
            return;
        }
        mounted.bounds = bounds;
        mounted.webview.set_bounds(to_wry_rect(bounds));
    }

    pub(crate) fn set_appearance(&mut self, entity: Entity, appearance: PageAppearance) {
        let Some(mounted) = self.pages.get_mut(&entity) else {
            return;
        };
        if mounted.appearance == appearance {
            return;
        }
        mounted.appearance = appearance;
        mounted
            .webview
            .set_appearance(to_webview_appearance(appearance));
    }

    pub(crate) fn set_interactive(&mut self, entity: Entity, interactive: bool) -> Option<u64> {
        let mounted = self.pages.get(&entity)?;
        Some(mounted.webview.set_interactive(interactive))
    }

    pub(crate) fn deliver(&self, entity: Entity, id: &'static str, payload: &[u8]) -> bool {
        let Some(mounted) = self.pages.get(&entity) else {
            return false;
        };
        let delivered = mounted.webview.deliver(id, payload);
        if delivered {
            mounted.webview.render();
        }
        delivered
    }

    pub(crate) fn evaluate_script(&self, entity: Entity, script: &str) -> bool {
        let Some(mounted) = self.pages.get(&entity) else {
            return false;
        };
        mounted.webview.evaluate_script(script)
    }

    pub(crate) fn focus(&self, entity: Entity) -> bool {
        let Some(mounted) = self.pages.get(&entity) else {
            return false;
        };
        mounted.webview.focus()
    }

    pub(crate) fn render_all(&self) {
        for mounted in self.pages.values() {
            mounted.webview.render();
        }
    }

    pub(crate) fn generation_of(&self, entity: Entity) -> Option<u64> {
        self.pages.get(&entity).map(|mounted| mounted.generation)
    }
}

fn to_wry_rect(bounds: PageBounds) -> wry::Rect {
    wry::Rect {
        position: wry::dpi::LogicalPosition::new(bounds.left as f64, bounds.top as f64).into(),
        size: wry::dpi::LogicalSize::new(
            bounds.width.max(1.0) as f64,
            bounds.height.max(1.0) as f64,
        )
        .into(),
    }
}

fn to_webview_appearance(appearance: PageAppearance) -> Appearance {
    match appearance {
        PageAppearance::Light => Appearance::Light,
        PageAppearance::Dark => Appearance::Dark,
        PageAppearance::System => Appearance::System,
    }
}

struct WinitPageWake {
    proxy: Option<EventLoopProxy<WinitUserEvent>>,
}

impl RendererWake for WinitPageWake {
    fn wake(&self) {
        if let Some(proxy) = &self.proxy {
            let _ = proxy.send_event(WinitUserEvent::WakeUp);
        }
    }

    fn task_waker(&self) -> std::task::Waker {
        vmux_runner::WinitWake::optional_waker(self.proxy.clone())
            .unwrap_or_else(|| std::task::Waker::noop().clone())
    }
}

struct StaticAssets {
    default_css: Option<String>,
    directory: Option<PathBuf>,
}

impl StaticAssets {
    const FALLBACK_CSS: &'static str =
        "html,body{margin:0;padding:0;height:100%;font-family:system-ui,-apple-system,sans-serif;}";

    fn resolve(&self, url: &str) -> Option<(String, Vec<u8>)> {
        let relative = Self::extract_path(url)?;
        if relative == "assets/index.css"
            && let Some(css) = &self.default_css
        {
            return Some((
                "text/css; charset=utf-8".to_string(),
                css.as_bytes().to_vec(),
            ));
        }
        if let Some(directory) = &self.directory
            && let Some(resolved) = Self::read_from_directory(directory, &relative)
        {
            return Some(resolved);
        }
        if relative == "assets/index.css" {
            return Some((
                "text/css; charset=utf-8".to_string(),
                Self::FALLBACK_CSS.as_bytes().to_vec(),
            ));
        }
        None
    }

    fn extract_path(url: &str) -> Option<String> {
        let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
        let without_query = after_scheme.split(['?', '#']).next().unwrap_or("");
        let relative = without_query
            .split_once('/')
            .map(|(_, rest)| rest)
            .unwrap_or("")
            .trim_start_matches('/');
        if relative.is_empty() {
            return None;
        }
        let decoded = Self::percent_decode(relative)?;
        let decoded = decoded.trim_start_matches('/');
        if decoded.is_empty() || decoded.contains('\0') {
            return None;
        }
        let path = Path::new(decoded);
        if path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return None;
        }
        Some(decoded.to_string())
    }

    fn percent_decode(input: &str) -> Option<String> {
        let bytes = input.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%' {
                if index + 2 >= bytes.len() {
                    return None;
                }
                let hi = (bytes[index + 1] as char).to_digit(16)?;
                let lo = (bytes[index + 2] as char).to_digit(16)?;
                out.push(((hi << 4) | lo) as u8);
                index += 3;
            } else {
                out.push(bytes[index]);
                index += 1;
            }
        }
        String::from_utf8(out).ok()
    }

    fn read_from_directory(root: &Path, relative: &str) -> Option<(String, Vec<u8>)> {
        let full = root.join(relative);
        let bytes = std::fs::read(&full).ok()?;
        Some((Self::mime_for(&full).to_string(), bytes))
    }

    fn mime_for(path: &Path) -> &'static str {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
        {
            "css" => "text/css; charset=utf-8",
            "js" | "mjs" => "application/javascript; charset=utf-8",
            "html" | "htm" => "text/html; charset=utf-8",
            "json" => "application/json; charset=utf-8",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "woff2" => "font/woff2",
            "woff" => "font/woff",
            "ttf" => "font/ttf",
            _ => "application/octet-stream",
        }
    }
}

impl RendererAssets for StaticAssets {
    fn fetch(&self, url: &str, reply: AssetReply) {
        match self.resolve(url) {
            Some((mime, bytes)) => reply.respond(200, &mime, bytes),
            None => reply.respond(404, "text/plain", Vec::new()),
        }
    }
}

struct ChannelOutbox {
    page: Entity,
    generation: u64,
    page_url: Arc<Mutex<String>>,
    sender: Sender<NativeEvent>,
    waker: Rc<dyn RendererWake>,
}

impl RendererOutbox for ChannelOutbox {
    fn send(
        &self,
        id: &str,
        bytes: &[u8],
        navigation_revision: u64,
        ipc_epoch: u64,
    ) -> Result<(), EventListenerError> {
        let page_url = self
            .page_url
            .lock()
            .map(|url| url.clone())
            .unwrap_or_default();
        let raw = UiReceiveRaw {
            page: self.page,
            id: id.to_string(),
            payload: bytes.to_vec(),
            page_url,
            generation: self.generation,
            navigation_revision,
            ipc_epoch,
        };
        if self
            .sender
            .send(NativeEvent {
                page: self.page,
                update: NativeUpdate::UiEvent(raw),
            })
            .is_err()
        {
            error!("vmux_page: native outbox receiver dropped");
        }
        self.waker.wake();
        Ok(())
    }

    fn set_page(&self, url: &str) {
        if let Ok(mut current) = self.page_url.lock() {
            *current = url.to_string();
        }
    }
}

struct ChannelLifecycle {
    page: Entity,
    sender: Sender<NativeEvent>,
    waker: Rc<dyn RendererWake>,
}

impl LifecycleSink for ChannelLifecycle {
    fn update(&self, update: LifecycleUpdate) {
        let _ = self.sender.send(NativeEvent {
            page: self.page,
            update: NativeUpdate::Lifecycle(update),
        });
        self.waker.wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_custom_stylesheet_directory_assets_and_blocks_traversal() {
        let temp = tempfile::tempdir().unwrap();
        let asset_dir = temp.path().join("assets");
        std::fs::create_dir_all(&asset_dir).unwrap();
        std::fs::write(asset_dir.join("app.js"), b"console.log(1);").unwrap();
        std::fs::write(asset_dir.join("my icon.svg"), b"<svg/>").unwrap();

        let assets = StaticAssets {
            default_css: Some("body{color:red;}".to_string()),
            directory: Some(temp.path().to_path_buf()),
        };

        let (mime, css) = assets.resolve("app://counter/assets/index.css").unwrap();
        assert!(mime.starts_with("text/css"));
        assert_eq!(css, b"body{color:red;}");

        let (js_mime, js) = assets.resolve("app://counter/assets/app.js?v=1").unwrap();
        assert!(js_mime.starts_with("application/javascript"));
        assert_eq!(js, b"console.log(1);");

        let (svg_mime, svg) = assets
            .resolve("app://counter/assets/my%20icon.svg")
            .unwrap();
        assert_eq!(svg_mime, "image/svg+xml");
        assert_eq!(svg, b"<svg/>");

        assert!(assets.resolve("app://counter/../secret.txt").is_none());
        assert!(
            assets
                .resolve("app://counter/assets/%2e%2e/secret.txt")
                .is_none()
        );

        let fallback = StaticAssets {
            default_css: None,
            directory: None,
        };
        assert!(fallback.resolve("app://counter/assets/index.css").is_some());
    }
}
