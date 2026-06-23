use crate::prelude::{
    BRP_PROMISES, LISTEN_EVENTS, PROCESS_MESSAGE_BIN_JS_EMIT, PROCESS_MESSAGE_BRP,
    PROCESS_MESSAGE_JS_EMIT,
};
use crate::util::{IntoString, v8_value_to_json};
use cef::rc::{Rc, RcImpl};
use cef::{
    CefString, ImplFrame, ImplListValue, ImplProcessMessage, ImplV8Context, ImplV8Handler,
    ImplV8Value, ProcessId, V8Value, WrapV8Handler, binary_value_create, process_message_create,
    sys, v8_context_get_current_context, v8_value_create_promise, v8_value_create_string,
};
use cef_dll_sys::cef_process_id_t;
use std::os::raw::c_int;

/// Handles the `window.cef` JavaScript API functions.
///
/// This handler is registered as a CEF extension during `on_web_kit_initialized`
/// and provides three native functions:
/// - `__cef_brp`: Async Bevy Remote Protocol requests
/// - `__cef_emit`: Send events from JavaScript to Bevy
/// - `__cef_listen`: Register callbacks for events from Bevy
///
/// The Frame is obtained dynamically via `v8_context_get_current_context().frame()`
/// since extensions are global and not bound to a specific context.
pub struct CefApiHandler {
    object: *mut RcImpl<sys::_cef_v8_handler_t, Self>,
}

impl Default for CefApiHandler {
    fn default() -> Self {
        Self {
            object: core::ptr::null_mut(),
        }
    }
}

impl Rc for CefApiHandler {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapV8Handler for CefApiHandler {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_v8_handler_t, Self>) {
        self.object = object;
    }
}

impl Clone for CefApiHandler {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self { object }
    }
}

impl ImplV8Handler for CefApiHandler {
    fn execute(
        &self,
        name: Option<&CefString>,
        _object: Option<&mut V8Value>,
        arguments: Option<&[Option<V8Value>]>,
        ret: Option<&mut Option<V8Value>>,
        _exception: Option<&mut CefString>,
    ) -> c_int {
        let Some(name) = name else { return 0 };
        let name_str = name.to_string();

        match name_str.as_str() {
            "__cef_brp" => self.execute_brp(arguments, ret),
            "__cef_emit" => self.execute_emit(arguments),
            "__cef_listen" => self.execute_listen(arguments),
            "__cef_bin_emit" => self.execute_bin_emit(arguments),
            _ => 0,
        }
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::_cef_v8_handler_t {
        self.object.cast()
    }
}

impl CefApiHandler {
    fn execute_brp(
        &self,
        arguments: Option<&[Option<V8Value>]>,
        ret: Option<&mut Option<V8Value>>,
    ) -> c_int {
        let Some(context) = v8_context_get_current_context() else {
            return 0;
        };
        let Some(frame) = context.frame() else {
            return 0;
        };
        if !crate::util::ipc_allowed_render(&frame.url().into_string()) {
            return 1;
        }

        if let Some(mut process) = process_message_create(Some(&PROCESS_MESSAGE_BRP.into()))
            && let Some(promise) = v8_value_create_promise()
        {
            if let Some(arguments_list) = process.argument_list()
                && let Some(arguments) = arguments
                && let Some(Some(arg)) = arguments.first()
                && let Some(brp_request) = v8_value_to_json(arg)
                && let Ok(brp_request) = serde_json::to_string(&brp_request)
                && let Some(ret) = ret
            {
                let id = uuid::Uuid::new_v4().to_string();
                arguments_list.set_string(0, Some(&id.as_str().into()));
                arguments_list.set_string(1, Some(&brp_request.as_str().into()));
                frame.send_process_message(
                    ProcessId::from(cef_process_id_t::PID_BROWSER),
                    Some(&mut process),
                );
                ret.replace(promise.clone());
                let mut promises = BRP_PROMISES.lock().unwrap();
                promises.insert(id, promise);
            } else {
                let mut exception =
                    v8_value_create_string(Some(&"Failed to execute BRP request".into()));
                promise.resolve_promise(exception.as_mut());
            }
        }
        1
    }

    fn execute_emit(&self, arguments: Option<&[Option<V8Value>]>) -> c_int {
        let Some(context) = v8_context_get_current_context() else {
            return 0;
        };
        let Some(frame) = context.frame() else {
            return 0;
        };
        if !crate::util::ipc_allowed_render(&frame.url().into_string()) {
            return 1;
        }

        if let Some(mut process) = process_message_create(Some(&PROCESS_MESSAGE_JS_EMIT.into()))
            && let Some(arguments_list) = process.argument_list()
            && let Some(arguments) = arguments
            && let Some(Some(arg)) = arguments.first()
            && let Some(arg) = v8_value_to_json(arg)
            && let Ok(arg) = serde_json::to_string(&arg)
        {
            crate::util::webview_debug_log(format!("render cef.emit payload_len={}", arg.len()));
            arguments_list.set_string(0, Some(&arg.as_str().into()));
            frame.send_process_message(
                ProcessId::from(cef_process_id_t::PID_BROWSER),
                Some(&mut process),
            );
        }
        1
    }

    fn execute_bin_emit(&self, arguments: Option<&[Option<V8Value>]>) -> c_int {
        let Some(context) = v8_context_get_current_context() else {
            return 0;
        };
        let Some(frame) = context.frame() else {
            return 0;
        };
        if !crate::util::ipc_allowed_render(&frame.url().into_string()) {
            return 1;
        }

        if let Some(mut process) = process_message_create(Some(&PROCESS_MESSAGE_BIN_JS_EMIT.into()))
            && let Some(arguments_list) = process.argument_list()
            && let Some(arguments) = arguments
            && let Some(Some(first)) = arguments.first()
        {
            let (id, payload_index) = if first.is_string().is_positive() {
                (first.string_value().into_string(), 1)
            } else {
                (String::new(), 0)
            };
            if !id.is_empty() {
                arguments_list.set_string(0, Some(&id.as_str().into()));
            }
            let Some(Some(arg)) = arguments.get(payload_index) else {
                return 1;
            };
            if !arg.is_array_buffer().is_positive() {
                return 1;
            }
            let len = arg.array_buffer_byte_length();
            if len == 0 {
                crate::util::webview_debug_log(format!(
                    "render cef.binEmit id={} payload_len=0 (no binary arg)",
                    id
                ));
                frame.send_process_message(
                    ProcessId::from(cef_process_id_t::PID_BROWSER),
                    Some(&mut process),
                );
                return 1;
            }
            let data_ptr = arg.array_buffer_data();
            if data_ptr.is_null() {
                return 1;
            }
            // SAFETY: V8 guarantees the ArrayBuffer backing store is valid for `len`
            // bytes for the duration of this synchronous call. Copy out before drop.
            let bytes = unsafe { std::slice::from_raw_parts(data_ptr.cast::<u8>(), len).to_vec() };

            if let Some(mut binary) = binary_value_create(Some(&bytes)) {
                let binary_index = if id.is_empty() { 0 } else { 1 };
                arguments_list.set_binary(binary_index, Some(&mut binary));
                crate::util::webview_debug_log(format!(
                    "render cef.binEmit id={} payload_len={len}",
                    id
                ));
                frame.send_process_message(
                    ProcessId::from(cef_process_id_t::PID_BROWSER),
                    Some(&mut process),
                );
            }
        }
        1
    }

    fn execute_listen(&self, arguments: Option<&[Option<V8Value>]>) -> c_int {
        let Some(context) = v8_context_get_current_context() else {
            return 1;
        };
        let Some(frame) = context.frame() else {
            return 1;
        };
        if !crate::util::ipc_allowed_render(&frame.url().into_string()) {
            return 1;
        }
        if let Some(arguments) = arguments
            && let Some(Some(id)) = arguments.first()
            && id.is_string().is_positive()
            && let Some(Some(callback)) = arguments.get(1)
            && callback.is_function().is_positive()
        {
            LISTEN_EVENTS
                .lock()
                .unwrap()
                .insert(id.string_value().into_string(), callback.clone());
            crate::util::webview_debug_log("render cef.listen registered");
        }
        1
    }
}
