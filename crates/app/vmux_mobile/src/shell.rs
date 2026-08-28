use std::cell::RefCell;

use bevy_app::{App, Last, Plugin};
use bevy_ecs::prelude::*;
use bevy_window::{AppLifecycle, PrimaryWindow};
use bevy_winit::{EventLoopProxyWrapper, WINIT_WINDOWS};
use vmux_native::{Instance, NativePage, WebView};

use crate::runtime::World as PageWorld;
use crate::surface::{PageWaker, Surfaces, embedding};

pub static SHELL_PAGE: NativePage = NativePage {
    url: "vmux://shell/",
    document_url: None,
    component: crate::Shell,
    root_id: "main",
    root_class: "flex min-h-0 min-w-0 flex-1 flex-col",
    head: r#"<base href="/"/>
<title>Vmux</title>
<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover"/>
<meta name="color-scheme" content="light dark"/>
<style>
html, body { height: 100%; margin: 0; min-height: 0; }
body { display: flex; flex-direction: column; min-height: 0; overflow: hidden; background: transparent; }
</style>
<link rel="stylesheet" href="./assets/index.css"/>
<link rel="stylesheet" href="./assets/theme.css"/>"#,
    html_attributes: r#"lang="en" class="h-full" style="color-scheme: light dark""#,
    body_class: "m-0 flex h-full min-h-0 flex-col overflow-hidden bg-transparent p-0 \
                 text-foreground antialiased",
    transparent: true,
    owns_subtree: false,
};

thread_local! {
    static MOUNTED: RefCell<Option<WebView>> = const { RefCell::new(None) };
}

pub(crate) struct ShellPlugin(pub &'static NativePage);

#[derive(Resource)]
struct Root(&'static NativePage);

impl Plugin for ShellPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Root(self.0))
            .add_systems(Last, (Shell::mount, Shell::pump).chain());
    }
}

struct Shell;

impl Shell {
    fn mount(
        windows: Query<Entity, With<PrimaryWindow>>,
        proxy: Option<Res<EventLoopProxyWrapper>>,
        root: Res<Root>,
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
                root.0,
                &**window,
                wry::Rect::default(),
                embedding(waker),
                Instance::of(|_| {}),
            ))
        });
        match built {
            Some(Ok(shell)) => {
                shell.order_among_siblings(vmux_native::SiblingOrder::Front);
                Self::adopt(entity, &shell);
                MOUNTED.with_borrow_mut(|slot| *slot = Some(shell));
            }
            Some(Err(error)) => tracing::error!(%error, "shell: the chrome would not mount"),
            None => tracing::debug!("shell: no winit window yet"),
        }
    }

    #[cfg(target_os = "ios")]
    fn adopt(entity: Entity, shell: &WebView) {
        let uikit = WINIT_WINDOWS.with(|windows| {
            let windows = windows.borrow();
            Uikit::of(&**windows.get_window(entity)?)
        });
        let Some(uikit) = uikit else {
            tracing::error!("shell: the window has no root view controller to adopt");
            return;
        };
        shell.fill_parent();
        uikit.paint_background();
        crate::deep_link::adopt();
        crate::transition::install(&uikit.controller, &uikit.view, &shell.ui_view());
        crate::qr_scanner::install(&uikit.controller);
    }

    #[cfg(not(target_os = "ios"))]
    fn adopt(_: Entity, _: &WebView) {}

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
            let Some(shell) = slot.as_ref() else {
                return;
            };
            shell.render();
        });
    }
}

#[cfg(target_os = "ios")]
const LIGHT_BACKGROUND: (u8, u8, u8, u8) = (215, 215, 215, 255);
#[cfg(target_os = "ios")]
const DARK_BACKGROUND: (u8, u8, u8, u8) = (10, 10, 10, 255);

#[cfg(target_os = "ios")]
fn webview_background() -> (u8, u8, u8, u8) {
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

    fn paint_background(&self) {
        use objc2_ui_kit::UIColor;

        let (red, green, blue, _) = webview_background();
        let color = UIColor::colorWithRed_green_blue_alpha(
            f64::from(red) / 255.0,
            f64::from(green) / 255.0,
            f64::from(blue) / 255.0,
            1.0,
        );
        self.window.setBackgroundColor(Some(&color));
    }
}
