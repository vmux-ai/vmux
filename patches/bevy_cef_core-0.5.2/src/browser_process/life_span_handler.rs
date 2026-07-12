use async_channel::Sender;
use bevy::prelude::Entity;
use cef::rc::{Rc, RcImpl};
use cef::{
    Browser, BrowserSettings, CefString, Client, DictionaryValue, Frame, ImplLifeSpanHandler,
    LifeSpanHandler, PopupFeatures, WindowInfo, WindowOpenDisposition, WrapLifeSpanHandler, sys,
};
use std::os::raw::c_int;

use crate::browser_process::renderer_handler::TextureWake;

#[derive(Clone, Debug)]
pub struct WebviewPopupEvent {
    pub webview: Entity,
    pub target_url: String,
}

pub type WebviewPopupSenderInner = Sender<WebviewPopupEvent>;

pub struct LifeSpanHandlerBuilder {
    object: *mut RcImpl<sys::_cef_life_span_handler_t, Self>,
    webview: Entity,
    tx: WebviewPopupSenderInner,
    wake: Option<TextureWake>,
}

impl LifeSpanHandlerBuilder {
    pub fn build(
        webview: Entity,
        tx: WebviewPopupSenderInner,
        wake: Option<TextureWake>,
    ) -> LifeSpanHandler {
        LifeSpanHandler::new(Self {
            object: core::ptr::null_mut(),
            webview,
            tx,
            wake,
        })
    }
}

impl Rc for LifeSpanHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            core::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for LifeSpanHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self {
            object,
            webview: self.webview,
            tx: self.tx.clone(),
            wake: self.wake.clone(),
        }
    }
}

impl WrapLifeSpanHandler for LifeSpanHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_life_span_handler_t, Self>) {
        self.object = object;
    }
}

impl ImplLifeSpanHandler for LifeSpanHandlerBuilder {
    #[allow(clippy::too_many_arguments)]
    fn on_before_popup(
        &self,
        _browser: Option<&mut Browser>,
        _frame: Option<&mut Frame>,
        _popup_id: c_int,
        target_url: Option<&CefString>,
        _target_frame_name: Option<&CefString>,
        _target_disposition: WindowOpenDisposition,
        _user_gesture: c_int,
        _popup_features: Option<&PopupFeatures>,
        _window_info: Option<&mut WindowInfo>,
        _client: Option<&mut Option<Client>>,
        _settings: Option<&mut BrowserSettings>,
        _extra_info: Option<&mut Option<DictionaryValue>>,
        _no_javascript_access: Option<&mut c_int>,
    ) -> c_int {
        let url = target_url.map(|u| u.to_string()).unwrap_or_default();
        let _ = self.tx.send_blocking(WebviewPopupEvent {
            webview: self.webview,
            target_url: url,
        });
        1
    }

    fn on_before_close(&self, _browser: Option<&mut Browser>) {
        if let Some(wake) = &self.wake {
            wake();
        }
    }

    fn get_raw(&self) -> *mut sys::_cef_life_span_handler_t {
        self.object.cast()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn browser_close_wakes_bevy() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let wakes_for_callback = Arc::clone(&wakes);
        let wake: TextureWake = Arc::new(move || {
            wakes_for_callback.fetch_add(1, Ordering::Relaxed);
        });
        let (tx, _) = async_channel::unbounded();
        let handler = LifeSpanHandlerBuilder {
            object: core::ptr::null_mut(),
            webview: Entity::PLACEHOLDER,
            tx,
            wake: Some(wake),
        };

        handler.on_before_close(None);

        assert_eq!(wakes.load(Ordering::Relaxed), 1);
    }
}
