#![allow(non_snake_case)]

use crate::event::{
    HardwareButton, SIMULATOR_READY_EVENT, SimulatorClipboard, SimulatorClipboardAction,
    SimulatorKey, SimulatorKeyModifiers, SimulatorReady, SimulatorSoftwareKeyboard, SimulatorTouch,
    SimulatorTouchPhase,
};
use crate::url::SimulatorRoute;
use dioxus::html::geometry::ClientPoint;
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use std::rc::Rc;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::hooks::{send, use_event, use_theme};
use vmux_ui::i18n::translate;
use vmux_ui::platform::sleep_ms;
use vmux_ui::script::PageScript;

#[component]
pub fn Page() -> Element {
    use_theme();
    let ready = use_event::<SimulatorReady>(SIMULATOR_READY_EVENT, SimulatorReady::default);
    let route = try_consume_context::<vmux_core::PageMetadata>()
        .and_then(|metadata| SimulatorRoute::of_url(&metadata.url));

    let announced = ready();
    rsx! {
        div { class: "flex h-screen w-screen items-center justify-center overflow-hidden bg-background",
            if announced.port == 0 {
                Waiting { route }
            } else {
                Mirror {
                    key: "{announced.port}-{announced.capability}",
                    port: announced.port,
                    capability: announced.capability.clone(),
                    device_name: announced.device_name.clone(),
                    frame_width: announced.frame_width,
                    frame_height: announced.frame_height,
                }
            }
        }
    }
}

#[component]
fn Mirror(
    port: u16,
    capability: String,
    device_name: String,
    frame_width: u32,
    frame_height: u32,
) -> Element {
    let mut press = use_signal(|| None::<PointerSession>);
    let mut image_size = use_signal(|| None::<(f64, f64)>);
    let mut home_progress = use_signal(|| 0.0f32);
    let mut surface = use_signal(|| None::<Rc<MountedData>>);
    let progress = home_progress();
    let scale = 1.0 - progress * 0.12;
    let offset = -progress * 18.0;
    let radius = progress * 28.0;
    let transition = if press().is_some_and(PointerSession::is_home) {
        "none"
    } else {
        "transform 180ms cubic-bezier(0.2, 0.8, 0.2, 1), border-radius 180ms ease-out"
    };
    let image_style = format!(
        "transform:translateY({offset:.2}px) scale({scale:.4});border-radius:{radius:.2}px;transition:{transition};"
    );
    let frame = SimulatorFrame::new(frame_width, frame_height);
    let phone_style = frame.phone_style();
    let screen_style = frame.screen_style();
    let stream = CanvasStream::new(port, capability, frame_width, frame_height);
    let stream_start = stream.clone();
    use_effect(move || {
        PageScript::run(stream_start.start_script());
    });
    let stream_stop = stream.clone();
    use_drop(move || {
        PageScript::run(stream_stop.stop_script());
    });
    rsx! {
        div {
            class: "relative flex h-full w-full items-center justify-center overflow-hidden bg-zinc-950/70 p-8 outline-none",
            tabindex: 0,
            onmounted: move |event: Event<MountedData>| {
                let target = event.data();
                surface.set(Some(target.clone()));
                spawn(async move {
                    if let Err(error) = target.set_focus(true).await {
                        dioxus::logger::tracing::warn!("focusing the simulator failed: {error:?}");
                    }
                });
            },
            onpointerdown: move |_| {
                let Some(target) = surface.peek().clone() else {
                    return;
                };
                spawn(async move {
                    if let Err(error) = target.set_focus(true).await {
                        dioxus::logger::tracing::warn!("focusing the simulator failed: {error:?}");
                    }
                });
            },
            onkeydown: move |event| {
                if SoftwareKeyboardShortcut::matches(&event) {
                    event.prevent_default();
                    let _ = send(&SimulatorSoftwareKeyboard);
                    return;
                }
                if let Some(action) = ClipboardShortcut::from_event(&event) {
                    event.prevent_default();
                    let _ = send(&SimulatorClipboard { action });
                    return;
                }
                let Some(key) = Keystroke::from_event(&event) else {
                    return;
                };
                event.prevent_default();
                let _ = send(&key);
            },
            onpointermove: move |event: Event<PointerData>| {
                let Some(current) = press() else {
                    return;
                };
                event.prevent_default();
                if !event.held_buttons().contains(MouseButton::Primary) {
                    press.set(None);
                    current
                        .release_at(event.client_coordinates())
                        .dispatch(home_progress);
                    return;
                }
                let Some((next, touch)) = current.move_to(event.client_coordinates()) else {
                    return;
                };
                press.set(Some(next));
                home_progress.set(next.home_progress());
                if let Some(touch) = touch {
                    let _ = send(&touch);
                }
            },
            onpointerup: move |event: Event<PointerData>| {
                let Some(current) = press.take() else {
                    return;
                };
                event.prevent_default();
                current
                    .release_at(event.client_coordinates())
                    .dispatch(home_progress);
            },
            onpointercancel: move |_| {
                let Some(current) = press.take() else {
                    return;
                };
                current.cancel().dispatch(home_progress);
            },
            div {
                id: stream.device_label_id.clone(),
                class: "pointer-events-none absolute left-5 top-4 text-sm font-medium text-zinc-300 opacity-0 transition-opacity",
                "{device_name}"
            }
            div {
                class: "relative bg-gradient-to-b from-zinc-700 via-zinc-950 to-black p-[7px] shadow-[0_28px_80px_rgba(0,0,0,0.65)] ring-1 ring-white/20",
                style: phone_style,
                div { class: "absolute -left-[3px] top-28 h-16 w-[3px] rounded-l bg-zinc-700" }
                div { class: "absolute -left-[3px] top-48 h-24 w-[3px] rounded-l bg-zinc-700" }
                div { class: "absolute -right-[3px] top-36 h-24 w-[3px] rounded-r bg-zinc-700" }
                div {
                    class: "relative overflow-hidden bg-black ring-1 ring-black",
                    style: screen_style,
                    SimulatorScreenSkeleton { id: stream.loader_id.clone() }
                    canvas {
                        id: "{stream.canvas_id}",
                        width: "{frame_width}",
                        height: "{frame_height}",
                        class: "block h-auto w-full cursor-grab touch-none select-none opacity-0 transition-opacity active:cursor-grabbing",
                        style: image_style,
                        onmounted: move |event: Event<MountedData>| {
                            let target = event.data();
                            spawn(async move {
                                let Ok(rect) = target.get_client_rect().await else {
                                    return;
                                };
                                image_size.set(Some((rect.size.width, rect.size.height)));
                            });
                        },
                        onresize: move |event: Event<ResizeData>| {
                            let Ok(size) = event.get_border_box_size() else {
                                return;
                            };
                            image_size.set(Some((size.width, size.height)));
                        },
                        onpointerdown: move |event: Event<PointerData>| {
                            if event
                                .trigger_button()
                                .is_some_and(|button| button != MouseButton::Primary)
                            {
                                return;
                            }
                            let Some(size) = image_size() else {
                                return;
                            };
                            event.prevent_default();
                            let Some((session, touch)) = PointerSession::start(&event, size) else {
                                return;
                            };
                            press.set(Some(session));
                            home_progress.set(0.0);
                            if let Some(touch) = touch {
                                let _ = send(&touch);
                            }
                        },
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
struct CanvasStream {
    canvas_id: String,
    loader_id: String,
    device_label_id: String,
    port: u16,
    capability: String,
    width: u32,
    height: u32,
}

impl CanvasStream {
    fn new(port: u16, capability: String, width: u32, height: u32) -> Self {
        Self {
            canvas_id: format!("simulator-stream-{capability}"),
            loader_id: format!("simulator-loader-{capability}"),
            device_label_id: format!("simulator-device-{capability}"),
            port,
            capability,
            width,
            height,
        }
    }

    fn start_script(&self) -> String {
        format!(
            r#"
const key = "{canvas_id}";
const streams = globalThis.__vmuxSimulatorStreams ??= new Map();
streams.get(key)?.abort();
const controller = new AbortController();
streams.set(key, controller);
const reveal = () => {{
  document.getElementById("{loader_id}")?.classList.add("hidden");
  document.getElementById("{canvas_id}")?.classList.replace("opacity-0", "opacity-100");
  const label = document.getElementById("{device_label_id}");
  label?.classList.remove("opacity-0");
  label?.classList.add("opacity-100");
}};
(async () => {{
try {{
  const canvas = document.getElementById(key);
  const context = canvas?.getContext("2d", {{ alpha: false, desynchronized: true }});
  if (!canvas || !context) throw new Error("simulator canvas is unavailable");
  let generation = 0;
  let first = true;
  while (!controller.signal.aborted) {{
    const frameUrl = `/__simulator-frame?port={port}&capability={capability}&after=${{generation}}`;
    const response = await fetch(frameUrl, {{ signal: controller.signal, cache: "no-store" }});
    if (!response.ok) throw new Error(`simulator stream failed: ${{response.status}}`);
    const payload = new Uint8Array(await response.arrayBuffer());
    if (payload.length < 9) throw new Error("simulator frame was empty");
    const generationView = new DataView(payload.buffer, payload.byteOffset, 8);
    generation = generationView.getUint32(0, true) + generationView.getUint32(4, true) * 4294967296;
    const bitmap = await createImageBitmap(new Blob([payload.subarray(8)], {{ type: "image/jpeg" }}));
    if (!canvas.isConnected) {{
      bitmap.close();
      controller.abort();
      break;
    }}
    context.drawImage(bitmap, 0, 0, {width}, {height});
    bitmap.close();
    if (first) {{
      first = false;
      reveal();
    }}
  }}
}} catch (error) {{
  if (!controller.signal.aborted) {{
    console.error(error);
  }}
}} finally {{
  if (streams.get(key) === controller) streams.delete(key);
}}
}})();
"#,
            canvas_id = self.canvas_id,
            loader_id = self.loader_id,
            device_label_id = self.device_label_id,
            port = self.port,
            capability = self.capability,
            width = self.width,
            height = self.height,
        )
    }

    fn stop_script(&self) -> String {
        format!(
            r#"
const streams = globalThis.__vmuxSimulatorStreams;
const controller = streams?.get("{}");
controller?.abort();
streams?.delete("{}");
"#,
            self.canvas_id, self.canvas_id,
        )
    }
}

struct SoftwareKeyboardShortcut;

impl SoftwareKeyboardShortcut {
    fn matches(event: &Event<KeyboardData>) -> bool {
        let modifiers = event.modifiers();
        modifiers.meta()
            && !modifiers.ctrl()
            && !modifiers.alt()
            && !modifiers.shift()
            && event.key().to_string().eq_ignore_ascii_case("k")
    }
}

struct ClipboardShortcut;

impl ClipboardShortcut {
    fn from_event(event: &Event<KeyboardData>) -> Option<SimulatorClipboardAction> {
        let modifiers = event.modifiers();
        if !modifiers.meta() || modifiers.ctrl() || modifiers.alt() || modifiers.shift() {
            return None;
        }
        Self::for_key(&event.key().to_string())
    }

    fn for_key(key: &str) -> Option<SimulatorClipboardAction> {
        match key.to_ascii_lowercase().as_str() {
            "a" => Some(SimulatorClipboardAction::SelectAll),
            "c" => Some(SimulatorClipboardAction::Copy),
            "x" => Some(SimulatorClipboardAction::Cut),
            "v" => Some(SimulatorClipboardAction::Paste),
            _ => None,
        }
    }
}

struct Keystroke;

impl Keystroke {
    fn from_event(event: &Event<KeyboardData>) -> Option<SimulatorKey> {
        let modifiers = event.modifiers();
        let key = event.key().to_string();
        if modifiers.meta() && !modifiers.ctrl() && !modifiers.alt() && !modifiers.shift() {
            return match key.to_ascii_lowercase().as_str() {
                "h" => Some(SimulatorKey::Button(HardwareButton::Home)),
                "l" => Some(SimulatorKey::Button(HardwareButton::Lock)),
                "s" => Some(SimulatorKey::Button(HardwareButton::Siri)),
                _ => SimulatorKey::modified_browser_code(
                    &event.code().to_string(),
                    Self::modifiers(event),
                ),
            };
        }
        let modifiers = Self::modifiers(event);
        if !modifiers.is_empty() {
            return SimulatorKey::modified_browser_code(&event.code().to_string(), modifiers);
        }
        SimulatorKey::of_browser_key(&key)
    }

    fn modifiers(event: &Event<KeyboardData>) -> SimulatorKeyModifiers {
        let modifiers = event.modifiers();
        SimulatorKeyModifiers {
            control: modifiers.ctrl(),
            shift: modifiers.shift(),
            alt: modifiers.alt(),
            meta: modifiers.meta(),
        }
    }
}

#[component]
fn Waiting(route: Option<SimulatorRoute>) -> Element {
    let _ = route;
    let frame = SimulatorFrame::fallback();
    rsx! {
        div { class: "relative flex h-full w-full items-center justify-center overflow-hidden bg-zinc-950/70 p-8",
            div {
                class: "relative bg-gradient-to-b from-zinc-700 via-zinc-950 to-black p-[7px] shadow-[0_28px_80px_rgba(0,0,0,0.65)] ring-1 ring-white/20",
                style: frame.phone_style(),
                div { class: "absolute -left-[3px] top-28 h-16 w-[3px] rounded-l bg-zinc-700" }
                div { class: "absolute -left-[3px] top-48 h-24 w-[3px] rounded-l bg-zinc-700" }
                div { class: "absolute -right-[3px] top-36 h-24 w-[3px] rounded-r bg-zinc-700" }
                div {
                    class: "relative overflow-hidden bg-black ring-1 ring-black",
                    style: frame.screen_style(),
                    SimulatorScreenSkeleton {}
                }
            }
        }
    }
}

#[component]
fn SimulatorScreenSkeleton(#[props(default)] id: String) -> Element {
    rsx! {
        div {
            id,
            class: "absolute inset-0 z-10 flex flex-col overflow-hidden bg-zinc-950 p-[5%]",
            role: "status",
            aria_label: translate("simulator-title"),
            div { class: "flex items-center justify-between",
                Skeleton { class: "h-2.5 w-[18%] rounded-full bg-white/[0.08]" }
                div { class: "flex gap-1.5",
                    Skeleton { class: "size-2.5 rounded-full bg-white/[0.08]" }
                    Skeleton { class: "h-2.5 w-5 rounded-full bg-white/[0.08]" }
                }
            }
            Skeleton { class: "mt-[8%] h-[5%] w-[44%] rounded-full bg-white/[0.07]" }
            Skeleton { class: "mt-[4%] h-[18%] w-full rounded-[8%] bg-white/[0.055]" }
            div { class: "mt-[5%] grid grid-cols-2 gap-[4%]",
                Skeleton { class: "aspect-square rounded-[10%] bg-white/[0.045]" }
                Skeleton { class: "aspect-square rounded-[10%] bg-white/[0.045]" }
            }
            Skeleton { class: "mt-[6%] h-[4%] w-[32%] rounded-full bg-white/[0.07]" }
            div { class: "mt-[4%] flex flex-col gap-3",
                Skeleton { class: "h-10 w-full rounded-xl bg-white/[0.045]" }
                Skeleton { class: "h-10 w-full rounded-xl bg-white/[0.045]" }
                Skeleton { class: "h-10 w-full rounded-xl bg-white/[0.045]" }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct SimulatorFrame {
    width: u32,
    height: u32,
}

impl SimulatorFrame {
    fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    fn fallback() -> Self {
        Self::new(603, 1311)
    }

    fn phone_style(self) -> String {
        let ratio = f64::from(self.width) / f64::from(self.height.max(1));
        format!(
            "width:min({}px,calc((100vh - 5rem) * {ratio:.8}),calc(100vw - 5rem));border-radius:14.5% / 6.7%;",
            self.width,
        )
    }

    fn screen_style(self) -> String {
        format!(
            "aspect-ratio:{} / {};border-radius:13% / 6%;",
            self.width, self.height,
        )
    }
}

#[derive(Clone, Copy)]
struct PointerSession {
    origin: (f64, f64),
    size: (f64, f64),
    start: (f32, f32),
    last: (f32, f32),
    home_candidate: bool,
    home: bool,
    touch_started: bool,
}

impl PointerSession {
    const HOME_ACTIVATION_DISTANCE: f32 = 0.03;
    const HOME_COMPLETION_DISTANCE: f32 = 0.08;
    const HOME_START_Y: f32 = 0.88;

    fn start(
        event: &Event<PointerData>,
        size: (f64, f64),
    ) -> Option<(Self, Option<SimulatorTouch>)> {
        let client = event.client_coordinates();
        let local = event.element_coordinates();
        let origin = (client.x - local.x, client.y - local.y);
        let point = Self::fraction((client.x, client.y), origin, size)?;
        let home_candidate = point.1 >= Self::HOME_START_Y;
        let session = Self {
            origin,
            size,
            start: point,
            last: point,
            home_candidate,
            home: false,
            touch_started: !home_candidate,
        };
        let touch = session
            .touch_started
            .then(|| session.touch(SimulatorTouchPhase::Down));
        Some((session, touch))
    }

    fn move_to(mut self, point: ClientPoint) -> Option<(Self, Option<SimulatorTouch>)> {
        let point = Self::fraction((point.x, point.y), self.origin, self.size)?;
        if point == self.last {
            return None;
        }
        self.last = point;
        if !self.home && self.activates_home() {
            self.home = true;
            return Some((self, None));
        }
        if self.home_candidate && self.rejects_home() {
            self.home_candidate = false;
            self.home = false;
            self.touch_started = true;
            return Some((
                self,
                Some(Self::touch_at(self.start, SimulatorTouchPhase::Down)),
            ));
        }
        let touch = self
            .touch_started
            .then(|| self.touch(SimulatorTouchPhase::Move));
        Some((self, touch))
    }

    fn release_at(mut self, point: ClientPoint) -> PointerRelease {
        if let Some(point) = Self::fraction((point.x, point.y), self.origin, self.size) {
            self.last = point;
        }
        if !self.home {
            if !self.touch_started {
                return PointerRelease::Touch(Self::touch_at(self.start, SimulatorTouchPhase::Tap));
            }
            return PointerRelease::Touch(self.touch(SimulatorTouchPhase::Up));
        }
        if self.completes_home() {
            PointerRelease::Home
        } else {
            PointerRelease::None
        }
    }

    fn cancel(self) -> PointerRelease {
        if self.home || !self.touch_started {
            PointerRelease::None
        } else {
            PointerRelease::Touch(self.touch(SimulatorTouchPhase::Cancel))
        }
    }

    fn touch(self, phase: SimulatorTouchPhase) -> SimulatorTouch {
        Self::touch_at(self.last, phase)
    }

    fn touch_at(point: (f32, f32), phase: SimulatorTouchPhase) -> SimulatorTouch {
        SimulatorTouch {
            phase,
            x: point.0,
            y: point.1,
        }
    }

    fn is_home(self) -> bool {
        self.home
    }

    fn home_progress(self) -> f32 {
        if !self.home {
            return 0.0;
        }
        ((self.start.1 - self.last.1) / 0.35).clamp(0.0, 1.0)
    }

    fn completes_home(self) -> bool {
        let dx = self.last.0 - self.start.0;
        let dy = self.last.1 - self.start.1;
        dy < -Self::HOME_COMPLETION_DISTANCE && dy.abs() > dx.abs()
    }

    fn activates_home(self) -> bool {
        if !self.home_candidate {
            return false;
        }
        let dx = self.last.0 - self.start.0;
        let dy = self.last.1 - self.start.1;
        dy < -Self::HOME_ACTIVATION_DISTANCE && dy.abs() > dx.abs()
    }

    fn rejects_home(self) -> bool {
        if !self.home_candidate {
            return false;
        }
        let dx = self.last.0 - self.start.0;
        let dy = self.last.1 - self.start.1;
        let distance = (dx.powi(2) + dy.powi(2)).sqrt();
        distance >= Self::HOME_ACTIVATION_DISTANCE && (dy >= 0.0 || dx.abs() >= dy.abs())
    }

    fn fraction(point: (f64, f64), origin: (f64, f64), size: (f64, f64)) -> Option<(f32, f32)> {
        if size.0 <= 0.0 || size.1 <= 0.0 {
            return None;
        }
        Some((
            ((point.0 - origin.0) / size.0).clamp(0.0, 1.0) as f32,
            ((point.1 - origin.1) / size.1).clamp(0.0, 1.0) as f32,
        ))
    }
}

enum PointerRelease {
    Touch(SimulatorTouch),
    Home,
    None,
}

impl PointerRelease {
    fn dispatch(self, mut home_progress: Signal<f32>) {
        match self {
            Self::Touch(touch) => {
                home_progress.set(0.0);
                let _ = send(&touch);
            }
            Self::Home => {
                home_progress.set(1.0);
                let _ = send(&SimulatorKey::Button(HardwareButton::Home));
                spawn(async move {
                    sleep_ms(300).await;
                    home_progress.set(0.0);
                });
            }
            Self::None => home_progress.set(0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rejected_home_gesture_finishes_its_fallback_touch() {
        let session = PointerSession {
            origin: (0.0, 0.0),
            size: (100.0, 100.0),
            start: (0.5, 0.95),
            last: (0.5, 0.95),
            home_candidate: true,
            home: false,
            touch_started: false,
        };
        let (session, touch) = session
            .move_to(ClientPoint::new(50.0, 90.0))
            .expect("home activation");
        assert!(session.home);
        assert!(touch.is_none());

        let (session, touch) = session
            .move_to(ClientPoint::new(70.0, 90.0))
            .expect("fallback touch");
        assert!(!session.home);
        assert_eq!(
            touch.map(|touch| touch.phase),
            Some(SimulatorTouchPhase::Down)
        );
        assert!(matches!(
            session.release_at(ClientPoint::new(70.0, 90.0)),
            PointerRelease::Touch(SimulatorTouch {
                phase: SimulatorTouchPhase::Up,
                ..
            })
        ));
    }

    #[test]
    fn command_edit_shortcuts_are_forwarded_to_the_simulator() {
        assert_eq!(
            ClipboardShortcut::for_key("a"),
            Some(SimulatorClipboardAction::SelectAll)
        );
        assert_eq!(
            ClipboardShortcut::for_key("x"),
            Some(SimulatorClipboardAction::Cut)
        );
        assert_eq!(
            ClipboardShortcut::for_key("c"),
            Some(SimulatorClipboardAction::Copy)
        );
        assert_eq!(
            ClipboardShortcut::for_key("v"),
            Some(SimulatorClipboardAction::Paste)
        );
    }
}
