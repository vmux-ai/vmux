use crate::prelude::{CefExtensions, EXTENSIONS_SWITCH, MessageLoopTimer};
use cef::rc::{Rc, RcImpl};
use cef::*;
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};
use winit::event_loop::EventLoopProxy;

pub type WakeProxy = EventLoopProxy<bevy_winit::WinitUserEvent>;

const WAKE_MIN_INTERVAL: Duration = Duration::from_millis(33);

/// ## Reference
///
/// - [`CefBrowserProcessHandler Class Reference`](https://cef-builds.spotifycdn.com/docs/106.1/classCefBrowserProcessHandler.html)
pub struct BrowserProcessHandlerBuilder {
    object: *mut RcImpl<cef_dll_sys::cef_browser_process_handler_t, Self>,
    message_loop_working_requester: Sender<MessageLoopTimer>,
    extensions: CefExtensions,
    wake_request_tx: Option<Sender<()>>,
}

impl BrowserProcessHandlerBuilder {
    pub fn build(
        message_loop_working_requester: Sender<MessageLoopTimer>,
        extensions: CefExtensions,
        wake_proxy: Option<WakeProxy>,
    ) -> BrowserProcessHandler {
        let wake_request_tx = wake_proxy.map(spawn_wake_throttler);

        BrowserProcessHandler::new(Self {
            object: core::ptr::null_mut(),
            message_loop_working_requester,
            extensions,
            wake_request_tx,
        })
    }
}

fn spawn_wake_throttler(proxy: WakeProxy) -> Sender<()> {
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("cef-wake-throttle".into())
        .spawn(move || {
            let mut last_fire: Option<Instant> = None;
            while rx.recv().is_ok() {
                if let Some(t) = last_fire {
                    let elapsed = Instant::now().duration_since(t);
                    if elapsed < WAKE_MIN_INTERVAL {
                        std::thread::sleep(WAKE_MIN_INTERVAL - elapsed);
                    }
                }
                while rx.try_recv().is_ok() {}
                let _ = proxy.send_event(bevy_winit::WinitUserEvent::WakeUp);
                last_fire = Some(Instant::now());
            }
        })
        .expect("failed to spawn cef-wake-throttle thread");
    tx
}

impl Rc for BrowserProcessHandlerBuilder {
    fn as_base(&self) -> &cef_dll_sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapBrowserProcessHandler for BrowserProcessHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<cef_dll_sys::_cef_browser_process_handler_t, Self>) {
        self.object = object;
    }
}

impl Clone for BrowserProcessHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            message_loop_working_requester: self.message_loop_working_requester.clone(),
            extensions: self.extensions.clone(),
            wake_request_tx: self.wake_request_tx.clone(),
        }
    }
}

impl ImplBrowserProcessHandler for BrowserProcessHandlerBuilder {
    fn on_before_child_process_launch(&self, command_line: Option<&mut CommandLine>) {
        let Some(command_line) = command_line else {
            return;
        };

        command_line.append_switch(Some(&"disable-web-security".into()));
        command_line.append_switch(Some(&"allow-running-insecure-content".into()));
        command_line.append_switch(Some(&"disable-session-crashed-bubble".into()));
        command_line.append_switch(Some(&"ignore-certificate-errors".into()));
        command_line.append_switch(Some(&"ignore-ssl-errors".into()));
        command_line.append_switch(Some(&"enable-logging=stderr".into()));
        command_line.append_switch(Some(&"disable-web-security".into()));
        // Pass extensions to render process via command line
        if !self.extensions.is_empty()
            && let Ok(json) = serde_json::to_string(&self.extensions.0)
        {
            command_line.append_switch_with_value(
                Some(&EXTENSIONS_SWITCH.into()),
                Some(&json.as_str().into()),
            );
        }
    }

    fn on_schedule_message_pump_work(&self, delay_ms: i64) {
        let _ = self
            .message_loop_working_requester
            .send(MessageLoopTimer::new(delay_ms));
        if let Some(tx) = &self.wake_request_tx {
            let _ = tx.send(());
        }
    }

    #[inline]
    fn get_raw(&self) -> *mut cef_dll_sys::_cef_browser_process_handler_t {
        self.object.cast()
    }
}
