use crate::prelude::{EXTENSIONS_SWITCH, IntoString};
use crate::render_process::cef_api_handler::CefApiHandler;
use crate::util::json_to_v8;
use crate::util::v8_accessor::V8DefaultAccessorBuilder;
use crate::util::v8_interceptor::V8DefaultInterceptorBuilder;
use cef::rc::{Rc, RcImpl};
use cef::{
    Browser, CefString, DictionaryValue, Frame, ImplBinaryValue, ImplBrowser, ImplCommandLine,
    ImplDictionaryValue, ImplFrame, ImplListValue, ImplProcessMessage, ImplRenderProcessHandler,
    ImplV8Context, ImplV8Exception, ImplV8Value, ProcessId, ProcessMessage, V8Context, V8Handler,
    V8Value, WrapRenderProcessHandler, command_line_get_global, process_message_create,
    register_extension, sys, v8_value_create_array_buffer_with_copy, v8_value_create_object,
    v8_value_create_string,
};
use std::collections::HashMap;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

const CEF_API_EXTENSION_NAME: &str = "v8/bevy-cef-api";
const CEF_API_EXTENSION_CODE: &str = r#"
var cef;
if (!cef) cef = {};
(function() {
  native function __cef_emit();
  native function __cef_listen();
  native function __cef_bin_emit();
  cef.emit = __cef_emit;
  cef.listen = __cef_listen;
  cef.binEmit = __cef_bin_emit;
  cef.binListen = __cef_listen;
})();
"#;

pub(crate) static LISTEN_EVENTS: LazyLock<Mutex<HashMap<(c_int, String, String), Vec<V8Value>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct InitScript {
    source: String,
    url: String,
}

static INIT_SCRIPTS: LazyLock<Mutex<HashMap<c_int, InitScript>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static BIN_SURFACE_GENERATIONS: LazyLock<Mutex<HashMap<c_int, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static BIN_IPC_EPOCHS: LazyLock<Mutex<HashMap<c_int, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_BIN_DOCUMENT_REVISION: AtomicU64 = AtomicU64::new(0);
pub const INIT_SCRIPT_KEY: &str = "init_script";
pub const INIT_SCRIPT_URL_KEY: &str = "init_script_url";
pub const BIN_IPC_SURFACE_GENERATION_KEY: &str = "bin_ipc_surface_generation";
pub const BIN_IPC_EPOCH_KEY: &str = "bin_ipc_epoch";

pub const PROCESS_MESSAGE_HOST_EMIT: &str = "host-emit";
pub const PROCESS_MESSAGE_JS_EMIT: &str = "js-emit";
pub const PROCESS_MESSAGE_BIN_HOST_EMIT: &str = "bin-host-emit";
pub const PROCESS_MESSAGE_BIN_JS_EMIT: &str = "bin-js-emit";
pub const PROCESS_MESSAGE_BIN_DOCUMENT_CONTEXT: &str = "bin-document-context";
pub const PROCESS_MESSAGE_BIN_IPC_EPOCH: &str = "bin-ipc-epoch";
pub const PROCESS_MESSAGE_SNAPSHOT: &str = "vmux-snapshot";
pub const PROCESS_MESSAGE_SNAPSHOT_RESULT: &str = "vmux-snapshot-result";

const BIN_DOCUMENT_IDENTITY_KEY: &str = "__vmuxBinDocumentIdentity";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BinDocumentIdentity {
    pub(crate) surface_generation: u64,
    pub(crate) navigation_revision: u64,
}

impl BinDocumentIdentity {
    fn next(surface_generation: u64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let mut current = NEXT_BIN_DOCUMENT_REVISION.load(Ordering::Acquire);
        let navigation_revision = loop {
            let next = now.max(current.wrapping_add(1).max(1));
            match NEXT_BIN_DOCUMENT_REVISION.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break next,
                Err(observed) => current = observed,
            }
        };
        Self {
            surface_generation,
            navigation_revision,
        }
    }

    fn bind(browser: &mut Browser, frame: &mut Frame, context: &mut V8Context) {
        if frame.is_main() == 0 || !crate::util::has_embedded_scheme(&frame.url().into_string()) {
            return;
        }
        let browser_id = browser.identifier();
        let Some(surface_generation) = BIN_SURFACE_GENERATIONS
            .lock()
            .ok()
            .and_then(|generations| generations.get(&browser_id).copied())
        else {
            return;
        };
        let identity = Self::next(surface_generation);
        if !identity.install(context) {
            return;
        }
        let Some(mut process) =
            process_message_create(Some(&PROCESS_MESSAGE_BIN_DOCUMENT_CONTEXT.into()))
        else {
            return;
        };
        let Some(arguments) = process.argument_list() else {
            return;
        };
        let token = identity.token();
        arguments.set_string(0, Some(&token.as_str().into()));
        frame.send_process_message(
            ProcessId::from(cef_dll_sys::cef_process_id_t::PID_BROWSER),
            Some(&mut process),
        );
    }

    fn install(self, context: &mut V8Context) -> bool {
        if context.enter() == 0 {
            return false;
        }
        let installed = context.global().is_some_and(|global| {
            let token = self.token();
            let mut identity = v8_value_create_string(Some(&token.as_str().into()));
            let attributes = sys::cef_v8_propertyattribute_t(
                [
                    sys::cef_v8_propertyattribute_t::V8_PROPERTY_ATTRIBUTE_READONLY,
                    sys::cef_v8_propertyattribute_t::V8_PROPERTY_ATTRIBUTE_DONTENUM,
                    sys::cef_v8_propertyattribute_t::V8_PROPERTY_ATTRIBUTE_DONTDELETE,
                ]
                .into_iter()
                .fold(0, |value, attribute| value | attribute.0),
            )
            .into();
            global.set_value_bykey(
                Some(&BIN_DOCUMENT_IDENTITY_KEY.into()),
                identity.as_mut(),
                attributes,
            ) != 0
        });
        context.exit();
        installed
    }

    pub(crate) fn current_token(context: &V8Context) -> Option<String> {
        let global = context.global()?;
        let identity = global.value_bykey(Some(&BIN_DOCUMENT_IDENTITY_KEY.into()))?;
        if !identity.is_string().is_positive() {
            return None;
        }
        let token = identity.string_value().into_string();
        Self::from_token(&token)?;
        Some(token)
    }

    fn token(self) -> String {
        format!(
            "{:016x}{:016x}",
            self.surface_generation, self.navigation_revision
        )
    }

    fn from_token(token: &str) -> Option<Self> {
        if token.len() != 32 {
            return None;
        }
        let surface_generation = u64::from_str_radix(&token[..16], 16).ok()?;
        let navigation_revision = u64::from_str_radix(&token[16..], 16).ok()?;
        (surface_generation != 0 && navigation_revision != 0).then_some(Self {
            surface_generation,
            navigation_revision,
        })
    }
}

pub(crate) fn current_bin_ipc_epoch(context: &V8Context) -> Option<u64> {
    let browser = context.browser()?;
    BIN_IPC_EPOCHS
        .lock()
        .ok()
        .and_then(|epochs| epochs.get(&browser.identifier()).copied())
}

pub struct RenderProcessHandlerBuilder {
    object: *mut RcImpl<sys::_cef_render_process_handler_t, Self>,
}

impl RenderProcessHandlerBuilder {
    pub fn build() -> RenderProcessHandlerBuilder {
        RenderProcessHandlerBuilder {
            object: core::ptr::null_mut(),
        }
    }
}

impl WrapRenderProcessHandler for RenderProcessHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_render_process_handler_t, Self>) {
        self.object = object;
    }
}

impl Rc for RenderProcessHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for RenderProcessHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self { object }
    }
}

impl ImplRenderProcessHandler for RenderProcessHandlerBuilder {
    fn on_web_kit_initialized(&self) {
        register_cef_api_extension();
        register_extensions_from_command_line();
    }

    fn on_browser_created(
        &self,
        browser: Option<&mut Browser>,
        extra: Option<&mut DictionaryValue>,
    ) {
        if let (Some(browser), Some(extra)) = (browser, extra) {
            let id = browser.identifier();
            if let Ok(surface_generation) = extra
                .string(Some(&BIN_IPC_SURFACE_GENERATION_KEY.into()))
                .into_string()
                .parse::<u64>()
            {
                BIN_SURFACE_GENERATIONS
                    .lock()
                    .unwrap()
                    .insert(id, surface_generation);
            }
            if let Ok(ipc_epoch) = extra
                .string(Some(&BIN_IPC_EPOCH_KEY.into()))
                .into_string()
                .parse::<u64>()
            {
                BIN_IPC_EPOCHS.lock().unwrap().insert(id, ipc_epoch);
            }
            let script = extra.string(Some(&INIT_SCRIPT_KEY.into())).into_string();
            if script.is_empty() {
                return;
            }
            let url = extra
                .string(Some(&INIT_SCRIPT_URL_KEY.into()))
                .into_string();
            INIT_SCRIPTS.lock().unwrap().insert(
                id,
                InitScript {
                    source: script,
                    url,
                },
            );
        }
    }

    fn on_context_created(
        &self,
        browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        context: Option<&mut V8Context>,
    ) {
        if let Some(context) = context
            && let Some(frame) = frame
            && let Some(browser) = browser
        {
            BinDocumentIdentity::bind(browser, frame, context);
            inject_initialize_scripts(browser, context, frame);
        }
    }

    fn on_context_released(
        &self,
        browser: Option<&mut Browser>,
        _frame: Option<&mut Frame>,
        context: Option<&mut V8Context>,
    ) {
        let Some(browser) = browser else {
            return;
        };
        let Some(context) = context else {
            return;
        };
        let Some(token) = BinDocumentIdentity::current_token(context) else {
            return;
        };
        if let Ok(mut events) = LISTEN_EVENTS.lock() {
            let browser_id = browser.identifier();
            events.retain(|(registered_browser_id, registered_token, _), _| {
                *registered_browser_id != browser_id || registered_token != &token
            });
        }
    }

    fn on_browser_destroyed(&self, browser: Option<&mut Browser>) {
        if let Some(browser) = browser {
            let browser_id = browser.identifier();
            if let Ok(mut events) = LISTEN_EVENTS.lock() {
                events.retain(|(registered_browser_id, _, _), _| {
                    *registered_browser_id != browser_id
                });
            }
            if let Ok(mut scripts) = INIT_SCRIPTS.lock() {
                scripts.remove(&browser_id);
            }
            if let Ok(mut generations) = BIN_SURFACE_GENERATIONS.lock() {
                generations.remove(&browser_id);
            }
            if let Ok(mut epochs) = BIN_IPC_EPOCHS.lock() {
                epochs.remove(&browser_id);
            }
        }
    }

    fn on_process_message_received(
        &self,
        _browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        _: ProcessId,
        message: Option<&mut ProcessMessage>,
    ) -> c_int {
        let name = message.as_ref().map(|m| m.name().into_string());
        if name.as_deref() == Some(PROCESS_MESSAGE_SNAPSHOT) {
            if let (Some(message), Some(frame)) = (message, frame) {
                let request_id = message
                    .argument_list()
                    .map(|args| args.string(0).into_string())
                    .unwrap_or_default();
                crate::dom_snapshot::request_dom_snapshot(&*frame, &request_id);
            }
            return 1;
        }
        if let Some(browser) = _browser
            && let Some(message) = message
            && let Some(frame) = frame
            && crate::util::has_embedded_scheme(&frame.url().into_string())
        {
            let browser_id = browser.identifier();
            match message.name().into_string().as_str() {
                PROCESS_MESSAGE_HOST_EMIT => {
                    if let Some(ctx) = frame.v8_context() {
                        handle_listen_message(message, browser_id, ctx);
                    }
                }
                PROCESS_MESSAGE_BIN_HOST_EMIT => {
                    if let Some(ctx) = frame.v8_context() {
                        handle_bin_listen_message(message, browser_id, ctx);
                    }
                }
                PROCESS_MESSAGE_BIN_IPC_EPOCH => {
                    if let Some(arguments) = message.argument_list()
                        && let Ok(epoch) = arguments.string(0).into_string().parse::<u64>()
                    {
                        BIN_IPC_EPOCHS.lock().unwrap().insert(browser_id, epoch);
                    }
                }
                _ => {}
            }
        };
        1
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::_cef_render_process_handler_t {
        self.object.cast()
    }
}

fn inject_initialize_scripts(browser: &mut Browser, context: &mut V8Context, frame: &mut Frame) {
    let id = browser.identifier();
    let frame_url = frame.url().into_string();
    if let Some(script) = INIT_SCRIPTS.lock().ok().and_then(|scripts| {
        let script = scripts.get(&id)?;
        should_inject_init_script(&script.url, &frame_url, frame.is_main() != 0)
            .then(|| CefString::from(script.source.as_str()))
    }) {
        context.enter();
        let mut retval: Option<V8Value> = None;
        let mut exception: Option<cef::V8Exception> = None;
        let result = context.eval(
            Some(&script),
            Some(&(&frame.url()).into()),
            0,
            Some(&mut retval),
            Some(&mut exception),
        );
        if result == 0 {
            if let Some(ex) = exception {
                eprintln!(
                    "bevy_cef: eval failed - message: {}, line: {}, column: {}",
                    ex.message().into_string(),
                    ex.line_number(),
                    ex.start_column(),
                );
            } else {
                eprintln!("bevy_cef: eval failed with no exception details");
            }
        } else {
        }
        context.exit();
    }
}

fn should_inject_init_script(expected_url: &str, frame_url: &str, is_main: bool) -> bool {
    is_main && expected_url == frame_url
}

#[cfg(test)]
mod tests {
    use super::should_inject_init_script;

    #[test]
    fn init_script_is_scoped_to_initial_main_frame() {
        let expected = "chrome-extension://aaaaaaaa/vmux_bridge.html";
        assert!(should_inject_init_script(expected, expected, true));
        assert!(!should_inject_init_script(expected, expected, false));
        assert!(!should_inject_init_script(
            expected,
            "https://example.com/",
            true
        ));
    }
}

fn register_cef_api_extension() {
    register_extension(
        Some(&CEF_API_EXTENSION_NAME.into()),
        Some(&CEF_API_EXTENSION_CODE.into()),
        Some(&mut V8Handler::new(CefApiHandler::default())),
    );
}

fn handle_listen_message(message: &ProcessMessage, browser_id: c_int, mut ctx: V8Context) {
    let Some(argument_list) = message.argument_list() else {
        return;
    };
    let Some(token) = authorized_host_message(&argument_list, &ctx) else {
        return;
    };
    let id = argument_list.string(0).into_string();
    let payload = argument_list.string(1).into_string();

    let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) else {
        return;
    };

    // Host can emit before JS calls `cef.listen` (e.g. WASM boot). Do not call `ctx.enter()` or
    // allocate V8 objects when there is no listener — that triggered Blink "HandleScope" fatals.
    let callbacks = {
        let Ok(events) = LISTEN_EVENTS.lock() else {
            return;
        };
        events.get(&(browser_id, token, id)).cloned()
    };
    let Some(callbacks) = callbacks else {
        return;
    };

    if ctx.enter().is_positive() {
        for callback in callbacks {
            let mut obj = v8_value_create_object(
                Some(&mut V8DefaultAccessorBuilder::build()),
                Some(&mut V8DefaultInterceptorBuilder::build()),
            );
            if let Some(arg) = json_to_v8(value.clone()) {
                let _ = callback.execute_function_with_context(
                    Some(&mut ctx),
                    obj.as_mut(),
                    Some(&[Some(arg)]),
                );
            }
        }
        ctx.exit();
    }
}

fn handle_bin_listen_message(message: &ProcessMessage, browser_id: c_int, mut ctx: V8Context) {
    let Some(argument_list) = message.argument_list() else {
        return;
    };
    let Some(token) = authorized_host_message(&argument_list, &ctx) else {
        return;
    };
    let id = argument_list.string(0).into_string();
    // CEF rejects zero-length BinaryValue, so empty payloads arrive without arg 1.
    // Treat absent binary as empty bytes.
    let mut buffer = match argument_list.binary(1) {
        Some(binary) => {
            let len = binary.size();
            let mut buf = vec![0u8; len];
            binary.data(Some(&mut buf), 0);
            buf
        }
        None => Vec::new(),
    };
    let len = buffer.len();

    let callbacks = {
        let Ok(events) = LISTEN_EVENTS.lock() else {
            return;
        };
        events.get(&(browser_id, token, id)).cloned()
    };
    let Some(callbacks) = callbacks else {
        return;
    };

    if ctx.enter().is_positive() {
        for callback in callbacks {
            let array_buffer = v8_value_create_array_buffer_with_copy(buffer.as_mut_ptr(), len);
            let mut this = v8_value_create_object(
                Some(&mut V8DefaultAccessorBuilder::build()),
                Some(&mut V8DefaultInterceptorBuilder::build()),
            );
            let _ = callback.execute_function_with_context(
                Some(&mut ctx),
                this.as_mut(),
                Some(&[array_buffer]),
            );
        }
        ctx.exit();
    }
}

fn authorized_host_message(arguments: &cef::ListValue, context: &V8Context) -> Option<String> {
    let token = arguments.string(2).into_string();
    if BinDocumentIdentity::current_token(context).as_deref() != Some(token.as_str()) {
        return None;
    }
    let epoch = arguments.string(3).into_string().parse::<u64>().ok()?;
    (current_bin_ipc_epoch(context) == Some(epoch)).then_some(token)
}

fn register_extensions_from_command_line() {
    let Some(cmd_line) = command_line_get_global() else {
        return;
    };
    if cmd_line.has_switch(Some(&EXTENSIONS_SWITCH.into())) == 0 {
        return;
    }
    let json = cmd_line
        .switch_value(Some(&EXTENSIONS_SWITCH.into()))
        .into_string();
    if json.is_empty() {
        return;
    }

    let Ok(extensions) = serde_json::from_str::<HashMap<String, String>>(&json) else {
        eprintln!("bevy_cef: failed to parse extensions JSON: {}", json);
        return;
    };

    for (name, code) in extensions {
        let full_name = format!("v8/{}", name);
        register_extension(
            Some(&full_name.as_str().into()),
            Some(&code.as_str().into()),
            None,
        );
    }
}
