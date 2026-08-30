use std::cell::RefCell;
use std::sync::OnceLock;

use bevy_app::{App, Last, Plugin};
use bevy_ecs::prelude::*;
use bevy_window::{AppLifecycle, PrimaryWindow};
use bevy_winit::{EventLoopProxyWrapper, WINIT_WINDOWS};
use vmux_native::{Instance, NativePage, PageComponent, WebView};

use crate::runtime::World as PageWorld;
use crate::surface::{PageWaker, Surfaces, embedding};

thread_local! {
    static MOUNTED: RefCell<Option<WebView>> = const { RefCell::new(None) };
}

pub(crate) struct RootPlugin(&'static NativePage);

impl RootPlugin {
    pub(crate) fn around(root: PageComponent, backdrop: Option<(u8, u8, u8, u8)>) -> Self {
        static SHELL: OnceLock<NativePage> = OnceLock::new();
        Self(SHELL.get_or_init(|| {
            let page = NativePage::pane("vmux://app/", root)
                .heading(
                    r#"<base href="/"/>
<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover"/>
<meta name="color-scheme" content="light dark"/>
<style>
html, body { height: 100%; margin: 0; min-height: 0; }
body { display: flex; flex-direction: column; min-height: 0; overflow: hidden; background: transparent; }
</style>
<link rel="stylesheet" href="./assets/index.css"/>
<link rel="stylesheet" href="./assets/theme.css"/>"#,
                )
                .dressed(
                    r#"lang="en" class="h-full" style="color-scheme: light dark""#,
                    "m-0 flex h-full min-h-0 flex-col overflow-hidden bg-transparent p-0 \
                     text-foreground antialiased",
                )
                .see_through();
            match backdrop {
                Some(colour) => page.background(colour),
                None => page,
            }
        }))
    }
}

#[derive(Resource)]
struct Showing(&'static NativePage);

impl Plugin for RootPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Showing(self.0))
            .add_systems(Last, (Root::mount, Root::pump).chain());
    }
}

struct Root;

impl Root {
    fn mount(
        windows: Query<Entity, With<PrimaryWindow>>,
        proxy: Option<Res<EventLoopProxyWrapper>>,
        showing: Res<Showing>,
    ) {
        if MOUNTED.with_borrow(Option::is_some) {
            return;
        }
        let Ok(entity) = windows.single() else {
            return;
        };
        let waker = PageWaker::of(proxy.as_deref());
        Surfaces::wake_with(waker.clone());
        let built = WINIT_WINDOWS.with(|windows| {
            let windows = windows.borrow();
            let window = windows.get_window(entity)?;
            Some(WebView::build(
                showing.0,
                &**window,
                wry::Rect::default(),
                embedding(waker),
                Instance::of(|_| {}),
            ))
        });
        match built {
            Some(Ok(mounted)) => {
                mounted.order_among_siblings(vmux_native::SiblingOrder::Front);
                Self::adopt(entity, &mounted, showing.0);
                MOUNTED.with_borrow_mut(|slot| *slot = Some(mounted));
            }
            Some(Err(error)) => tracing::error!(%error, "root: the app page would not mount"),
            None => tracing::debug!("root: no winit window yet"),
        }
    }

    #[cfg(target_os = "ios")]
    fn adopt(entity: Entity, mounted: &WebView, page: &'static NativePage) {
        let uikit = WINIT_WINDOWS.with(|windows| {
            let windows = windows.borrow();
            Uikit::of(&**windows.get_window(entity)?)
        });
        let Some(uikit) = uikit else {
            tracing::error!("root: the window has no root view controller to adopt");
            return;
        };
        mounted.fill_parent();
        uikit.paint_background(page);
        crate::deep_link::adopt();
        crate::transition::install(&uikit.controller, &uikit.view, &mounted.ui_view(), page);
        crate::qr_scanner::install(&uikit.controller);
    }

    #[cfg(not(target_os = "ios"))]
    fn adopt(_: Entity, _: &WebView, _: &'static NativePage) {}

    fn pump(mut lifecycle: MessageReader<AppLifecycle>) {
        for reported in lifecycle.read() {
            if matches!(reported, AppLifecycle::WillResume) {
                crate::mark_resumed();
            }
            PageWorld::report(*reported);
        }
        PageWorld::with(PageWorld::tick);
        Self::render();
        crate::transition::NativeStack::render();
    }

    fn render() {
        MOUNTED.with_borrow(|slot| {
            let Some(mounted) = slot.as_ref() else {
                return;
            };
            mounted.render();
        });
    }
}

#[cfg(target_os = "ios")]
const LIGHT_BACKGROUND: (u8, u8, u8, u8) = (215, 215, 215, 255);
#[cfg(target_os = "ios")]
const DARK_BACKGROUND: (u8, u8, u8, u8) = (10, 10, 10, 255);

#[cfg(target_os = "ios")]
pub(crate) fn webview_background() -> (u8, u8, u8, u8) {
    use objc2_ui_kit::{UITraitCollection, UIUserInterfaceStyle};

    let style = unsafe { UITraitCollection::currentTraitCollection().userInterfaceStyle() };
    if style == UIUserInterfaceStyle::Dark {
        DARK_BACKGROUND
    } else {
        LIGHT_BACKGROUND
    }
}

#[cfg(target_os = "ios")]
struct Uikit {
    window: objc2::rc::Retained<objc2_ui_kit::UIWindow>,
    controller: objc2::rc::Retained<objc2_ui_kit::UIViewController>,
    view: objc2::rc::Retained<objc2_ui_kit::UIView>,
}

#[cfg(target_os = "ios")]
impl Uikit {
    fn of(window: &dyn wry::raw_window_handle::HasWindowHandle) -> Option<Self> {
        use objc2::rc::Retained;
        use objc2_ui_kit::UIView;
        use wry::raw_window_handle::RawWindowHandle;

        let handle = window.window_handle().ok()?;
        let RawWindowHandle::UiKit(uikit) = handle.as_raw() else {
            return None;
        };
        let view: Retained<UIView> = unsafe { Retained::retain(uikit.ui_view.as_ptr().cast())? };
        let window = view.window()?;
        let controller = window.rootViewController()?;
        Some(Self {
            window,
            controller,
            view,
        })
    }

    fn paint_background(&self, page: &'static NativePage) {
        use objc2_ui_kit::UIColor;

        let (red, green, blue, _) = page.background_or(webview_background());
        let color = UIColor::colorWithRed_green_blue_alpha(
            f64::from(red) / 255.0,
            f64::from(green) / 255.0,
            f64::from(blue) / 255.0,
            1.0,
        );
        self.window.setBackgroundColor(Some(&color));
    }
}
