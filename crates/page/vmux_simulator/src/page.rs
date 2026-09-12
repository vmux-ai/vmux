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
use vmux_ui::hooks::{send, use_event, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::matrix_rain::MatrixLoader;
use vmux_ui::platform::sleep_ms;

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
                }
            }
        }
    }
}

#[component]
fn Mirror(port: u16, capability: String, device_name: String) -> Element {
    let mut press = use_signal(|| None::<PointerSession>);
    let mut image_size = use_signal(|| None::<(f64, f64)>);
    let mut image_loaded = use_signal(|| false);
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
                if let Some(action) = ClipboardShortcut::of(&event) {
                    event.prevent_default();
                    let _ = send(&SimulatorClipboard { action });
                    return;
                }
                let Some(key) = Keystroke::of(&event) else {
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
            if !image_loaded() {
                SimulatorLoader { class: "absolute inset-0".to_string() }
            }
            div {
                class: if image_loaded() {
                    "pointer-events-none absolute left-5 top-4 text-sm font-medium text-zinc-300 opacity-100 transition-opacity"
                } else {
                    "pointer-events-none absolute left-5 top-4 text-sm font-medium text-zinc-300 opacity-0"
                },
                "{device_name}"
            }
            div { class: if image_loaded() {
                    "relative rounded-[3.25rem] bg-gradient-to-b from-zinc-700 via-zinc-950 to-black p-[7px] opacity-100 shadow-[0_28px_80px_rgba(0,0,0,0.65)] ring-1 ring-white/20 transition-opacity"
                } else {
                    "invisible relative rounded-[3.25rem] bg-gradient-to-b from-zinc-700 via-zinc-950 to-black p-[7px] opacity-0"
                },
                div { class: "absolute -left-[3px] top-28 h-16 w-[3px] rounded-l bg-zinc-700" }
                div { class: "absolute -left-[3px] top-48 h-24 w-[3px] rounded-l bg-zinc-700" }
                div { class: "absolute -right-[3px] top-36 h-24 w-[3px] rounded-r bg-zinc-700" }
                div { class: "overflow-hidden rounded-[2.8rem] bg-black ring-1 ring-black",
                    img {
                        class: "block h-auto max-h-[calc(100vh-5rem)] max-w-[calc(100vw-5rem)] cursor-grab touch-none select-none active:cursor-grabbing",
                        style: image_style,
                        draggable: false,
                        src: "http://127.0.0.1:{port}/{capability}",
                        onload: move |_| image_loaded.set(true),
                        onerror: move |_| image_loaded.set(false),
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
    fn of(event: &Event<KeyboardData>) -> Option<SimulatorClipboardAction> {
        let modifiers = event.modifiers();
        if !modifiers.meta() || modifiers.ctrl() || modifiers.alt() || modifiers.shift() {
            return None;
        }
        match event.key().to_string().to_ascii_lowercase().as_str() {
            "c" => Some(SimulatorClipboardAction::Copy),
            "v" => Some(SimulatorClipboardAction::Paste),
            _ => None,
        }
    }
}

struct Keystroke;

impl Keystroke {
    fn of(event: &Event<KeyboardData>) -> Option<SimulatorKey> {
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
    let label = match route {
        Some(SimulatorRoute::Pinned {
            version,
            device_name: Some(device_name),
        }) => translate_with(
            "simulator-waiting-device",
            &[
                ("device", TranslationValue::String(&device_name)),
                ("version", TranslationValue::String(version.as_str())),
            ],
        ),
        Some(SimulatorRoute::Pinned { version, .. }) => translate_with(
            "simulator-waiting-version",
            &[("version", TranslationValue::String(version.as_str()))],
        ),
        _ => translate("common-loading"),
    };
    rsx! {
        MatrixLoader {
            label,
            words: vec![translate("simulator-title").to_uppercase()],
        }
    }
}

#[component]
fn SimulatorLoader(class: String) -> Element {
    rsx! {
        MatrixLoader {
            class,
            label: translate("common-loading"),
            words: vec![translate("simulator-title").to_uppercase()],
        }
    }
}

#[derive(Clone, Copy)]
struct PointerSession {
    origin: (f64, f64),
    size: (f64, f64),
    start: (f32, f32),
    last: (f32, f32),
    home: bool,
}

impl PointerSession {
    fn start(
        event: &Event<PointerData>,
        size: (f64, f64),
    ) -> Option<(Self, Option<SimulatorTouch>)> {
        let client = event.client_coordinates();
        let local = event.element_coordinates();
        let origin = (client.x - local.x, client.y - local.y);
        let point = Self::fraction((client.x, client.y), origin, size)?;
        let home = point.1 >= 0.94;
        let session = Self {
            origin,
            size,
            start: point,
            last: point,
            home,
        };
        let touch = (!home).then(|| session.touch(SimulatorTouchPhase::Down));
        Some((session, touch))
    }

    fn move_to(mut self, point: ClientPoint) -> Option<(Self, Option<SimulatorTouch>)> {
        let point = Self::fraction((point.x, point.y), self.origin, self.size)?;
        if point == self.last {
            return None;
        }
        self.last = point;
        let touch = (!self.home).then(|| self.touch(SimulatorTouchPhase::Move));
        Some((self, touch))
    }

    fn release_at(mut self, point: ClientPoint) -> PointerRelease {
        if let Some(point) = Self::fraction((point.x, point.y), self.origin, self.size) {
            self.last = point;
        }
        if !self.home {
            return PointerRelease::Touch(self.touch(SimulatorTouchPhase::Up));
        }
        if self.completes_home() {
            PointerRelease::Home
        } else {
            PointerRelease::None
        }
    }

    fn cancel(self) -> PointerRelease {
        if self.home {
            PointerRelease::None
        } else {
            PointerRelease::Touch(self.touch(SimulatorTouchPhase::Cancel))
        }
    }

    fn touch(self, phase: SimulatorTouchPhase) -> SimulatorTouch {
        SimulatorTouch {
            phase,
            x: self.last.0,
            y: self.last.1,
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
        dy < -0.1 && dy.abs() > dx.abs()
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
