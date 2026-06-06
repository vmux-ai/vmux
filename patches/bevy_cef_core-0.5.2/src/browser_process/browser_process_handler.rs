use crate::prelude::{CefExtensions, EXTENSIONS_SWITCH, MessageLoopTimer, MessageLoopWakePolicy};
use cef::rc::{Rc, RcImpl};
use cef::*;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};
use winit::event_loop::EventLoopProxy;

pub type WakeProxy = EventLoopProxy<bevy_winit::WinitUserEvent>;

/// ## Reference
///
/// - [`CefBrowserProcessHandler Class Reference`](https://cef-builds.spotifycdn.com/docs/106.1/classCefBrowserProcessHandler.html)
pub struct BrowserProcessHandlerBuilder {
    object: *mut RcImpl<cef_dll_sys::cef_browser_process_handler_t, Self>,
    message_loop_working_requester: Sender<MessageLoopTimer>,
    extensions: CefExtensions,
    wake_request_tx: Option<Sender<MessageLoopTimer>>,
}

impl BrowserProcessHandlerBuilder {
    pub fn build(
        message_loop_working_requester: Sender<MessageLoopTimer>,
        extensions: CefExtensions,
        wake_proxy: Option<WakeProxy>,
        wake_policy: MessageLoopWakePolicy,
    ) -> BrowserProcessHandler {
        let wake_request_tx =
            wake_proxy.map(|proxy| spawn_wake_throttler(proxy, wake_policy.clone()));

        BrowserProcessHandler::new(Self {
            object: core::ptr::null_mut(),
            message_loop_working_requester,
            extensions,
            wake_request_tx,
        })
    }
}

fn spawn_wake_throttler(
    proxy: WakeProxy,
    policy: MessageLoopWakePolicy,
) -> Sender<MessageLoopTimer> {
    let (tx, rx) = mpsc::channel::<MessageLoopTimer>();
    std::thread::Builder::new()
        .name("cef-wake-throttle".into())
        .spawn(move || {
            let mut last_fire: Option<Instant> = None;
            while let Ok(timer) = rx.recv() {
                if wait_for_wake_deadline(&rx, timer, last_fire, &policy).is_none() {
                    break;
                }
                let _ = proxy.send_event(bevy_winit::WinitUserEvent::WakeUp);
                last_fire = Some(Instant::now());
            }
        })
        .expect("failed to spawn cef-wake-throttle thread");
    tx
}

fn wait_for_wake_deadline(
    rx: &mpsc::Receiver<MessageLoopTimer>,
    mut timer: MessageLoopTimer,
    last_fire: Option<Instant>,
    policy: &MessageLoopWakePolicy,
) -> Option<MessageLoopTimer> {
    loop {
        timer = drain_earliest_timer(rx, timer);
        let deadline = wake_deadline(timer, last_fire, policy.min_wake_interval());
        let now = Instant::now();
        if deadline <= now {
            return Some(timer);
        }
        match rx.recv_timeout(deadline - now) {
            Ok(next) => {
                timer = timer.earliest(next);
            }
            Err(RecvTimeoutError::Timeout) => return Some(timer),
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

fn drain_earliest_timer(
    rx: &mpsc::Receiver<MessageLoopTimer>,
    mut timer: MessageLoopTimer,
) -> MessageLoopTimer {
    while let Ok(next) = rx.try_recv() {
        timer = timer.earliest(next);
    }
    timer
}

fn wake_deadline(
    timer: MessageLoopTimer,
    last_fire: Option<Instant>,
    min_interval: Duration,
) -> Instant {
    let earliest_interval = last_fire
        .and_then(|instant| instant.checked_add(min_interval))
        .unwrap_or_else(Instant::now);
    timer.fire_time().max(earliest_interval)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cef_wake_throttle_uses_policy_interval() {
        let now = Instant::now();
        let timer = MessageLoopTimer::new(0);
        let deadline = wake_deadline(timer, Some(now), Duration::from_millis(250));

        assert!(deadline >= now + Duration::from_millis(250));
    }

    #[test]
    fn cef_wake_throttle_keeps_earliest_timer() {
        let (tx, rx) = mpsc::channel();
        let later = MessageLoopTimer::new(100);
        let earlier = MessageLoopTimer::new(0);
        tx.send(earlier).unwrap();

        assert_eq!(
            drain_earliest_timer(&rx, later).fire_time(),
            earlier.fire_time()
        );
    }
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
        let timer = MessageLoopTimer::new(delay_ms);
        let _ = self.message_loop_working_requester.send(timer);
        if let Some(tx) = &self.wake_request_tx {
            let _ = tx.send(timer);
        }
    }

    #[inline]
    fn get_raw(&self) -> *mut cef_dll_sys::_cef_browser_process_handler_t {
        self.object.cast()
    }
}
