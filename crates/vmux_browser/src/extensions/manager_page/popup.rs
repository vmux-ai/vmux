use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, HostWindow, JsEmitEventPlugin, Receive, UiEventPlugin, UiInput};
use vmux_core::event::{
    ExtensionPopupBoundsRequest, ExtensionPopupCloseRequest, ExtensionPopupEvent,
    ExtensionPopupOpenRequest, ExtensionPopupSizeEvent,
};
use vmux_core::extension::store;
use vmux_core::{KeyboardOwner, host::UiStateWrite};
use vmux_flex::prelude::Visibility;
use vmux_layout::{Browser, LayoutCef, state::LayoutUiState};

pub(super) struct PopupPlugin;

impl Plugin for PopupPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            ExtensionPopupOpenRequest,
            ExtensionPopupBoundsRequest,
            ExtensionPopupCloseRequest,
        )>::default())
            .add_plugins(JsEmitEventPlugin::<ExtensionPopupSizeRequest>::default())
            .add_observer(on_open_request)
            .add_observer(on_bounds_request)
            .add_observer(on_close_request)
            .add_observer(on_size)
            .add_systems(
                Update,
                inject_sizing.after(crate::page_life::drain_loading_state),
            );
    }
}

#[derive(Component)]
pub(crate) struct ExtensionPopup {
    pub(crate) owner: Entity,
    pub(crate) extension_id: String,
}

fn close_extension_popup(
    owner: Entity,
    popups: &Query<(Entity, &ExtensionPopup)>,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    let mut closed = false;
    for (entity, popup) in popups {
        if popup.owner != owner {
            continue;
        }
        browsers.hide_child_window(&entity);
        commands.entity(entity).try_despawn();
        closed = true;
    }
    if closed {
        commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
            owner,
            &ExtensionPopupEvent::default(),
        ));
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct ExtensionPopupBounds {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl ExtensionPopupBounds {
    fn from_request(request: &ExtensionPopupBoundsRequest) -> Option<Self> {
        if !request.left.is_finite()
            || !request.top.is_finite()
            || !request.width.is_finite()
            || !request.height.is_finite()
            || request.width <= 0.0
            || request.height <= 0.0
        {
            return None;
        }
        Some(Self {
            left: request.left,
            top: request.top,
            width: request.width,
            height: request.height,
        })
    }
}

#[derive(Component)]
pub(crate) struct ExtensionPopupPresented;

#[derive(serde::Deserialize)]
struct ExtensionPopupSizeRequest {
    channel: String,
    width: f32,
    height: f32,
}

const POPUP_SIZE_CHANNEL: &str = "vmux-extension-popup-size";

fn on_open_request(
    trigger: On<UiInput<ExtensionPopupOpenRequest>>,
    layouts: Query<(Entity, Option<&HostWindow>), With<LayoutCef>>,
    host_windows: Query<&HostWindow>,
    popups: Query<(Entity, &ExtensionPopup)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let id = trigger.event().payload.id.clone();
    let index = store::Index::load(&store::root()).unwrap_or_default();
    let Some(entry) = index.entries.into_iter().find(|entry| entry.id == id) else {
        return;
    };
    if !entry.enabled_for(&vmux_core::profile::active_profile_name()) {
        return;
    }
    let Some(popup) = entry.popup else {
        return;
    };
    let url = format!("chrome-extension://{id}/{popup}");
    let source = trigger.event().webview;
    let source_window = host_windows.get(source).ok().map(|host| host.0);
    let owner = layouts.iter().find_map(|(layout, host)| {
        (layout == source || source_window.is_none_or(|window| host.is_some_and(|h| h.0 == window)))
            .then_some(layout)
    });
    let Some(owner) = owner else {
        return;
    };
    close_extension_popup(owner, &popups, &browsers, &mut commands);
    commands
        .spawn(Browser::new_with_title(&url, &entry.name))
        .insert((
            Name::new(format!("Extension popup: {}", entry.name)),
            vmux_core::overlay::WindowOverlay,
            ExtensionPopup {
                owner,
                extension_id: id.clone(),
            },
            Visibility::Hidden,
        ));
    commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
        owner,
        &ExtensionPopupEvent {
            id,
            name: entry.name,
            icon: entry.icon,
            anchor: trigger.event().payload.anchor,
        },
    ));
}

fn on_bounds_request(
    trigger: On<UiInput<ExtensionPopupBoundsRequest>>,
    popups: Query<(Entity, &ExtensionPopup)>,
    mut commands: Commands,
) {
    let Some(bounds) = ExtensionPopupBounds::from_request(&trigger.event().payload) else {
        return;
    };
    for (entity, popup) in &popups {
        if popup.owner == trigger.event().webview {
            commands
                .entity(entity)
                .insert((bounds, Visibility::Visible, KeyboardOwner));
        }
    }
}

fn on_close_request(
    trigger: On<UiInput<ExtensionPopupCloseRequest>>,
    popups: Query<(Entity, &ExtensionPopup)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    close_extension_popup(trigger.event().webview, &popups, &browsers, &mut commands);
}

fn inject_sizing(
    mut events: MessageReader<crate::WebviewLoadCompleted>,
    popups: Query<(), With<ExtensionPopup>>,
    browsers: NonSend<Browsers>,
) {
    for event in events.read() {
        if !popups.contains(event.webview) {
            continue;
        }
        browsers.execute_js(
            &event.webview,
            r#"
(() => {
  if (globalThis.__vmuxPopupSizer) return;
  let scheduled = false;
  let lastWidth = 0;
  let lastHeight = 0;
  const measure = () => {
    scheduled = false;
    const body = document.body;
    if (!body) return;
    const root = document.documentElement;
    const viewportHeight = root.clientHeight || globalThis.innerHeight;
    const viewportWidth = root.clientWidth || globalThis.innerWidth;
    let height = 0;
    let width = 0;
    for (const element of body.querySelectorAll("*")) {
      const style = getComputedStyle(element);
      if (style.display === "none" || style.visibility === "hidden" || style.visibility === "collapse") {
        continue;
      }
      const rect = element.getBoundingClientRect();
      if (!Number.isFinite(rect.bottom) || !Number.isFinite(rect.right)) {
        continue;
      }
      const fillsHeight = rect.top <= 1 && Math.abs(rect.height - viewportHeight) <= 1 && element.scrollHeight <= element.clientHeight + 1;
      const fillsWidth = rect.left <= 1 && Math.abs(rect.width - viewportWidth) <= 1 && element.scrollWidth <= element.clientWidth + 1;
      if (!fillsHeight) {
        height = Math.max(height, rect.bottom);
      }
      if (!fillsWidth) {
        width = Math.max(width, rect.right);
      }
      if (element.scrollHeight > element.clientHeight + 1) {
        height = Math.max(height, rect.top + element.scrollHeight);
      }
      if (element.scrollWidth > element.clientWidth + 1) {
        width = Math.max(width, rect.left + element.scrollWidth);
      }
      resize.observe(element);
    }
    const style = getComputedStyle(body);
    height = Math.ceil(height + parseFloat(style.paddingBottom || 0) + parseFloat(style.marginBottom || 0));
    width = Math.ceil(width + parseFloat(style.paddingRight || 0) + parseFloat(style.marginRight || 0));
    if (height === lastHeight && width === lastWidth) return;
    lastHeight = height;
    lastWidth = width;
    cef.emit({ channel: "vmux-extension-popup-size", width, height });
  };
  const schedule = () => {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(measure);
  };
  const resize = new ResizeObserver(schedule);
  resize.observe(document.documentElement);
  resize.observe(document.body);
  const mutation = new MutationObserver(schedule);
  mutation.observe(document.body, { childList: true, subtree: true, attributes: true, characterData: true });
  globalThis.__vmuxPopupSizer = { resize, mutation };
  schedule();
})();
"#,
        );
    }
}

fn on_size(
    trigger: On<Receive<ExtensionPopupSizeRequest>>,
    popups: Query<&ExtensionPopup>,
    mut commands: Commands,
) {
    let request = &trigger.payload;
    if request.channel != POPUP_SIZE_CHANNEL
        || !request.width.is_finite()
        || !request.height.is_finite()
    {
        return;
    }
    let Ok(popup) = popups.get(trigger.event().webview) else {
        return;
    };
    commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
        popup.owner,
        &ExtensionPopupSizeEvent {
            id: popup.extension_id.clone(),
            width: request.width.clamp(200.0, 360.0),
            height: request.height.clamp(80.0, 600.0),
        },
    ));
}
