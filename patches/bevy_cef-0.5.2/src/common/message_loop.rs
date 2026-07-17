use crate::RunOnMainThread;
use crate::common::WebviewSource;
use bevy::prelude::*;
use bevy_cef_core::prelude::*;
use cef::args::Args;
use cef::{Settings, api_hash, execute_process, initialize, shutdown, sys};

/// Controls the CEF message loop.
///
/// Uses `external_message_pump` on all platforms.
pub struct MessageLoopPlugin {
    pub config: CommandLineConfig,
    pub extensions: CefExtensions,
    pub root_cache_path: Option<String>,
}

#[derive(Resource, Default)]
pub struct CefShutdownState {
    started: bool,
}

impl CefShutdownState {
    pub fn started(&self) -> bool {
        self.started
    }
}

impl Plugin for MessageLoopPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(target_os = "macos"))]
        let render_process_binary = render_process_path();

        #[cfg(target_os = "macos")]
        load_cef_library(app);

        let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
        let args = Args::new();
        let (tx, rx) = std::sync::mpsc::channel();

        // macOS pumps CEF from a native CFRunLoop timer (see below), so the pump must NOT wake
        // Bevy — leave the wake proxy unset there.
        #[cfg(target_os = "macos")]
        let wake_proxy: Option<WakeProxy> = None;
        #[cfg(not(target_os = "macos"))]
        let wake_proxy = app
            .world()
            .get_resource::<bevy::winit::EventLoopProxyWrapper>()
            .map(|wrapper| (**wrapper).clone());
        let wake_policy = MessageLoopWakePolicy::default();

        let mut cef_app = BrowserProcessAppBuilder::build(
            tx,
            self.config.clone(),
            self.extensions.clone(),
            wake_proxy,
            wake_policy.clone(),
        );

        // On macOS and when a separate render process binary is available,
        // execute_process is called here. For the browser process it returns -1
        // and falls through; subprocesses exit immediately.
        #[cfg(target_os = "macos")]
        {
            let ret = execute_process(
                Some(args.as_main_args()),
                Some(&mut cef_app),
                std::ptr::null_mut(),
            );
            if ret >= 0 {
                std::process::exit(ret);
            }
        }

        #[cfg(not(target_os = "macos"))]
        cef_initialize(
            &args,
            &mut cef_app,
            self.root_cache_path.as_deref(),
            render_process_binary.as_deref(),
        );
        #[cfg(target_os = "macos")]
        cef_initialize(&args, &mut cef_app, self.root_cache_path.as_deref());

        app.insert_resource(wake_policy.clone())
            .init_resource::<CefShutdownState>()
            .insert_non_send(cef_app)
            .insert_non_send(RunOnMainThread)
            .add_systems(
                Last,
                close_all_browsers_then_cef_shutdown
                    .after(bevy::window::ExitSystems)
                    .run_if(on_message::<AppExit>),
            );

        // non-macOS: pump CEF once per Bevy tick (the wake-throttle thread wakes the loop).
        #[cfg(not(target_os = "macos"))]
        app.insert_non_send(MessageLoopWorkingReceiver(rx))
            .add_systems(Main, cef_do_message_loop_work.before(Main::run_main));

        #[cfg(target_os = "macos")]
        {
            drop(rx);
            app.insert_non_send(cef_pump_timer::install(1.0 / 120.0));
        }
    }
}

#[cfg(target_os = "macos")]
fn load_cef_library(app: &mut App) {
    macos::install_cef_app_protocol();
    #[cfg(all(target_os = "macos", feature = "debug"))]
    let loader = DebugLibraryLoader::new();
    #[cfg(all(target_os = "macos", not(feature = "debug")))]
    let loader = cef::library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), false);
    assert!(loader.load());
    app.insert_non_send(loader);
}

#[cfg(target_os = "macos")]
fn cef_initialize(args: &Args, cef_app: &mut cef::App, root_cache_path: Option<&str>) {
    // Ensure the cache directory exists before CEF tries to use it.
    // Empty/whitespace paths are valid (CEF treats them as "use default"), so skip those.
    if let Some(path) = root_cache_path.filter(|p| !p.trim().is_empty()) {
        std::fs::create_dir_all(path)
            .unwrap_or_else(|e| panic!("failed to create root_cache_path directory '{path}': {e}"));
    }

    let browser_subprocess_path = {
        #[cfg(feature = "debug")]
        {
            let subprocess = debug_render_process_path();
            assert!(
                subprocess.is_file(),
                "CEF macOS debug render process missing at {}.\n\
The helper must live under the framework Libraries/ folder (not only in target/debug/): Chromium loads libGLESv2 and related dylibs relative to the subprocess path; a helper next to your app breaks GPU/subprocess startup and often yields ERR_UNKNOWN_URL_SCHEME for vmux://.\n\
Fix: make install-debug-render-process  (or: cargo build -p bevy_cef_debug_render_process && cp target/debug/bevy_cef_debug_render_process '{}')",
                subprocess.display(),
                debug_render_process_path().display(),
            );
            subprocess
                .to_str()
                .expect("debug render subprocess path must be UTF-8")
                .to_string()
        }
        #[cfg(not(feature = "debug"))]
        {
            let exe = std::env::current_exe().unwrap();
            let app_name = exe.file_name().unwrap().to_str().unwrap();
            let helper = exe
                .parent()
                .unwrap() // MacOS/
                .parent()
                .unwrap() // Contents/
                .join("Frameworks")
                .join(format!("{app_name} Helper.app"))
                .join("Contents")
                .join("MacOS")
                .join(format!("{app_name} Helper"));
            helper
                .to_str()
                .expect("helper subprocess path must be UTF-8")
                .to_string()
        }
    };

    let settings = Settings {
        #[cfg(feature = "debug")]
        framework_dir_path: debug_chromium_embedded_framework_dir_path()
            .to_str()
            .unwrap()
            .into(),
        browser_subprocess_path: browser_subprocess_path.as_str().into(),
        no_sandbox: true as _,
        root_cache_path: root_cache_path.unwrap_or_default().into(),
        background_color: 0x00000000,
        windowless_rendering_enabled: true as _,
        external_message_pump: true as _,
        disable_signal_handlers: true as _,
        ..Default::default()
    };
    assert_eq!(
        initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(cef_app),
            std::ptr::null_mut(),
        ),
        1,
        "cef_initialize failed: root_cache_path={root_cache_path:?}",
    );
}

#[cfg(not(target_os = "macos"))]
fn cef_initialize(
    args: &Args,
    cef_app: &mut cef::App,
    root_cache_path: Option<&str>,
    render_process_binary: Option<&std::path::Path>,
) {
    // Ensure the cache directory exists before CEF tries to use it.
    // Empty/whitespace paths are valid (CEF treats them as "use default"), so skip those.
    if let Some(path) = root_cache_path.filter(|p| !p.trim().is_empty()) {
        std::fs::create_dir_all(path)
            .unwrap_or_else(|e| panic!("failed to create root_cache_path directory '{path}': {e}"));
    }

    let subprocess_path: String = render_process_binary
        .and_then(|p| p.to_str())
        .unwrap_or_default()
        .into();

    let settings = Settings {
        browser_subprocess_path: subprocess_path.as_str().into(),
        no_sandbox: true as _,
        root_cache_path: root_cache_path.unwrap_or_default().into(),
        background_color: 0x00000000,
        windowless_rendering_enabled: true as _,
        external_message_pump: true as _,
        disable_signal_handlers: false as _,
        ..Default::default()
    };
    assert_eq!(
        initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(cef_app),
            std::ptr::null_mut(),
        ),
        1,
        "cef_initialize failed: root_cache_path={root_cache_path:?}, subprocess={subprocess_path:?}",
    );
}

// macOS pumps CEF from a CFRunLoop timer (see `cef_pump_timer`), so this Bevy-tick pump is only
// registered on other platforms.
#[cfg(not(target_os = "macos"))]
fn cef_do_message_loop_work(
    receiver: NonSend<MessageLoopWorkingReceiver>,
    mut timer: Local<Option<MessageLoopTimer>>,
    mut max_delay_timer: Local<MessageLoopWorkingMaxDelayTimer>,
) {
    while let Ok(t) = receiver.try_recv() {
        timer.replace(t);
    }
    if timer.as_ref().map(|t| t.is_finished()).unwrap_or(false) || max_delay_timer.is_finished() {
        *max_delay_timer = MessageLoopWorkingMaxDelayTimer::default();
        timer.take();
    }
    // Pump once per Bevy tick. Input handlers still do immediate targeted pumps after key events.
    cef::do_message_loop_work();
}

/// Close every open CEF browser before [`shutdown`]. Calling `cef::shutdown` while browsers still
/// exist can crash the process (e.g. quitting from the command bar or ⌘Q).
fn close_all_browsers_then_cef_shutdown(
    mut browsers: NonSendMut<Browsers>,
    mut exits: MessageReader<AppExit>,
    webviews: Query<Entity, With<WebviewSource>>,
    mut commands: Commands,
    mut state: ResMut<CefShutdownState>,
    mut pump_controller: Option<NonSendMut<CefPumpController>>,
    _: NonSend<RunOnMainThread>,
) {
    for _ in exits.read() {
        if state.started {
            continue;
        }
        state.started = true;
        let entities: Vec<Entity> = webviews.iter().collect();
        for e in entities {
            browsers.close(&e);
            commands.entity(e).despawn();
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while open_browser_count() > 0 && std::time::Instant::now() < deadline {
            cef::do_message_loop_work();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        if let Some(controller) = pump_controller.as_mut() {
            controller.stop();
        }
        if open_browser_count() == 0 {
            cef::do_message_loop_work();
            shutdown();
        } else {
            error!(
                "CEF shutdown skipped with {} browsers still closing",
                open_browser_count()
            );
        }
    }
}

#[cfg(target_os = "macos")]
type CefPumpController = cef_pump_timer::Controller;

#[cfg(not(target_os = "macos"))]
struct CefPumpController;

#[cfg(not(target_os = "macos"))]
impl CefPumpController {
    fn stop(&mut self) {}
}

#[allow(clippy::needless_doctest_main)]
/// On non-macOS platforms, this detects if the current process is a CEF subprocess
/// (renderer, GPU, utility) and exits immediately if so.
///
/// When no separate render process binary is installed, CEF re-launches the main
/// executable as a subprocess. Call this function at the very beginning of `main()`
/// — **before** any Bevy initialization — so that subprocess instances exit
/// immediately without creating a visible window.
///
/// ```no_run
/// fn main() {
///     bevy_cef::prelude::early_exit_if_subprocess();
///     // ... Bevy App setup ...
/// }
/// ```
///
/// If a dedicated render process binary (`bevy_cef_render_process`) is installed
/// next to your executable, this function is unnecessary because CEF will launch
/// that binary instead of re-using the main executable.
///
/// On macOS this function is not available; macOS always uses a separate render
/// process binary.
#[cfg(not(target_os = "macos"))]
pub fn early_exit_if_subprocess() {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = Args::new();
    let mut app = RenderProcessAppBuilder::build();
    let ret = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if ret >= 0 {
        std::process::exit(ret);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn main_message_loop_uses_single_pump_pass() {
        let main_loop = include_str!("message_loop.rs")
            .split("fn close_all_browsers_then_cef_shutdown")
            .next()
            .unwrap_or_default();
        assert_eq!(main_loop.matches("cef::do_message_loop_work();").count(), 1);
    }

    #[test]
    fn cef_global_background_is_transparent_for_windowed_glass() {
        let source = include_str!("message_loop.rs");

        assert!(source.contains("background_color: 0x00000000"));
    }
}

#[cfg(target_os = "macos")]
mod cef_pump_timer {
    use std::os::raw::c_void;

    type CFRunLoopRef = *mut c_void;
    type CFRunLoopTimerRef = *mut c_void;
    type CFStringRef = *const c_void;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopCommonModes: CFStringRef;
        fn CFRunLoopGetMain() -> CFRunLoopRef;
        fn CFAbsoluteTimeGetCurrent() -> f64;
        fn CFRunLoopAddTimer(rl: CFRunLoopRef, timer: CFRunLoopTimerRef, mode: CFStringRef);
        fn CFRunLoopRemoveTimer(rl: CFRunLoopRef, timer: CFRunLoopTimerRef, mode: CFStringRef);
        fn CFRunLoopTimerCreate(
            allocator: *const c_void,
            fire_date: f64,
            interval: f64,
            flags: u64,
            order: isize,
            callout: extern "C" fn(CFRunLoopTimerRef, *mut c_void),
            context: *mut c_void,
        ) -> CFRunLoopTimerRef;
        fn CFRunLoopTimerInvalidate(timer: CFRunLoopTimerRef);
        fn CFRelease(cf: *const c_void);
    }

    extern "C" fn pump(_timer: CFRunLoopTimerRef, _info: *mut c_void) {
        cef::do_message_loop_work();
    }

    pub struct Controller {
        run_loop: CFRunLoopRef,
        timer: CFRunLoopTimerRef,
        stopped: bool,
    }

    impl Controller {
        pub fn stop(&mut self) {
            if self.stopped {
                return;
            }
            self.stopped = true;
            unsafe {
                CFRunLoopRemoveTimer(self.run_loop, self.timer, kCFRunLoopCommonModes);
                CFRunLoopTimerInvalidate(self.timer);
                CFRelease(self.timer.cast_const());
            }
        }
    }

    impl Drop for Controller {
        fn drop(&mut self) {
            self.stop();
        }
    }

    pub fn install(interval: f64) -> Controller {
        unsafe {
            let timer = CFRunLoopTimerCreate(
                std::ptr::null(),
                CFAbsoluteTimeGetCurrent() + interval,
                interval,
                0,
                0,
                pump,
                std::ptr::null_mut(),
            );
            assert!(!timer.is_null(), "failed to create CEF CFRunLoop timer");
            let run_loop = CFRunLoopGetMain();
            CFRunLoopAddTimer(run_loop, timer, kCFRunLoopCommonModes);
            Controller {
                run_loop,
                timer,
                stopped: false,
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use core::sync::atomic::AtomicBool;
    use objc::runtime::{Class, Object, Sel};
    use objc::{sel, sel_impl};
    use std::os::raw::c_char;
    use std::os::raw::c_void;
    use std::sync::atomic::Ordering;

    unsafe extern "C" {
        fn class_addMethod(
            cls: *const Class,
            name: Sel,
            imp: *const c_void,
            types: *const c_char,
        ) -> bool;
    }

    static IS_HANDLING_SEND_EVENT: AtomicBool = AtomicBool::new(false);

    extern "C" fn is_handling_send_event(_: &Object, _: Sel) -> bool {
        IS_HANDLING_SEND_EVENT.load(Ordering::Relaxed)
    }

    extern "C" fn set_handling_send_event(_: &Object, _: Sel, flag: bool) {
        IS_HANDLING_SEND_EVENT.swap(flag, Ordering::Relaxed);
    }

    pub fn install_cef_app_protocol() {
        unsafe {
            let cls = Class::get("NSApplication").expect("NSApplication クラスが見つかりません");
            let sel_name = sel!(isHandlingSendEvent);
            let success = class_addMethod(
                cls as *const _,
                sel_name,
                is_handling_send_event as *const c_void,
                c"c@:".as_ptr() as *const c_char,
            );
            assert!(success, "メソッド追加に失敗しました");

            let sel_set = sel!(setHandlingSendEvent:);
            let success2 = class_addMethod(
                cls as *const _,
                sel_set,
                set_handling_send_event as *const c_void,
                c"v@:c".as_ptr() as *const c_char,
            );
            assert!(
                success2,
                "Failed to add setHandlingSendEvent: to NSApplication"
            );
        }
    }
}
