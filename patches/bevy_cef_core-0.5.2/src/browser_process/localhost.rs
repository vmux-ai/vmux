mod data_responser;
mod headers_responser;

use crate::browser_process::localhost::data_responser::{DataResponser, parse_bytes_single_range};
use crate::browser_process::localhost::headers_responser::HeadersResponser;
use crate::prelude::IntoString;
use async_channel::{Receiver, Sender};
use bevy::asset::Asset;
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use cef::rc::{Rc, RcImpl};
use cef::{
    Browser, Callback, CefString, Frame, ImplCallback, ImplRequest, ImplResourceHandler,
    ImplResponse, ImplSchemeHandlerFactory, Request, ResourceHandler, ResourceReadCallback,
    Response, SchemeHandlerFactory, WrapResourceHandler, WrapSchemeHandlerFactory, sys,
};
use cef_dll_sys::{_cef_resource_handler_t, cef_base_ref_counted_t};
use serde::{Deserialize, Serialize};
use std::os::raw::c_int;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::util::{CefEmbeddedPageConfig, resolved_cef_embedded_page_config, webview_debug_log};

/// Map navigated custom-scheme URLs to a Bevy [`AssetServer`] load path.
///
/// - `cef://localhost/embedded/…` → `embedded://…` ([embedded assets](https://bevy.org/examples/assets/embedded-asset/)).
/// - `cef://localhost/…` (otherwise) → path as-is (disk assets under the app asset root).
/// - `<scheme>://<host>/` (prefix from [`CefEmbeddedPageConfig::scheme_prefix`], set via
///   [`crate::util::try_set_cef_embedded_page_config`]) → `embedded://<default_document>` for that host.
/// - `<scheme>://<host>/<path>` → `embedded://<dir>/<path>` when `dir` is the parent of that host’s
///   `default_document` (e.g. default `history/index.html` → `embedded://history/<path>`); if the
///   default document has no parent directory, `embedded://<path>` only.
/// - Unknown `<host>`: `embedded://` + full path after the scheme prefix (first segment not in the table).
///
/// The render subprocess registers [`crate::util::compile_time_cef_embedded_scheme`] (defaults with the
/// `bevy_cef_core` build; override with env `BEVY_CEF_EMBEDDED_SCHEME` when building this crate). It must
/// match the runtime [`CefEmbeddedPageConfig::scheme`] from [`crate::util::try_set_cef_embedded_page_config`].
fn split_custom_scheme_host_and_tail(path_part: &str) -> Option<(&str, &str)> {
    let s = path_part.trim_start_matches('/');
    if s.is_empty() {
        return None;
    }
    match s.find('/') {
        Some(i) => {
            let host = s[..i].trim();
            let tail = s[i + 1..].trim_matches('/');
            if host.is_empty() {
                None
            } else {
                Some((host, tail))
            }
        }
        None => Some((s.trim(), "")),
    }
}

fn normalize_url_path_tail(path: &str) -> String {
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

pub(crate) fn asset_load_path_from_request_url_with(
    url: &str,
    cfg: &CefEmbeddedPageConfig,
) -> String {
    const CEF_LOCAL: &str = concat!("cef", "://", "localhost", "/");
    const EMBEDDED_LEAF: &str = "embedded/";
    const EMBEDDED_SCHEME: &str = "embedded://";

    if let Some(rest) = url.strip_prefix(CEF_LOCAL) {
        if let Some(tail) = rest.strip_prefix(EMBEDDED_LEAF) {
            format!("{EMBEDDED_SCHEME}{tail}")
        } else {
            rest.to_string()
        }
    } else if let Some(rest) = url.strip_prefix(cfg.scheme_prefix()) {
        let path_part = rest.split(['?', '#']).next().unwrap_or(rest);
        let Some((host, tail)) = split_custom_scheme_host_and_tail(path_part) else {
            return String::new();
        };
        if tail.starts_with(EMBEDDED_SCHEME) {
            return tail.to_string();
        }
        let tail = normalize_url_path_tail(tail);
        if let Some(entry) = cfg.hosts.entry_for_host(host) {
            if tail.is_empty() {
                format!("{EMBEDDED_SCHEME}{}", entry.default_document)
            } else {
                let rel = Path::new(entry.default_document.as_str())
                    .parent()
                    .and_then(|p| p.to_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && *s != ".")
                    .map(|base| base.trim_matches(['/', '\\']))
                    .filter(|s| !s.is_empty());
                match rel {
                    Some(base) => format!("{EMBEDDED_SCHEME}{base}/{tail}"),
                    None => format!("{EMBEDDED_SCHEME}{tail}"),
                }
            }
        } else {
            let full =
                normalize_url_path_tail(path_part.trim_start_matches('/').trim_end_matches('/'));
            if full.is_empty() {
                String::new()
            } else {
                format!("{EMBEDDED_SCHEME}{full}")
            }
        }
    } else {
        String::new()
    }
}

pub(crate) fn asset_load_path_from_request_url(url: &str) -> String {
    asset_load_path_from_request_url_with(url, resolved_cef_embedded_page_config().as_ref())
}

/// `cef://` scheme response asset.
#[derive(Asset, Reflect, Debug, Clone, Serialize, Deserialize)]
#[reflect(Debug, Serialize, Deserialize)]
pub struct CefResponse {
    /// The media type.
    pub mime_type: String,
    /// The status code of the response, e.g., 200 for OK, 404 for Not Found.
    pub status_code: u32,
    /// The response data, typically HTML or other content.
    pub data: Vec<u8>,
}

impl Default for CefResponse {
    fn default() -> Self {
        Self {
            mime_type: "text/html".to_string(),
            status_code: 404,
            data: b"<!DOCTYPE html><html><body><h1>404 Not Found</h1></body></html>".to_vec(),
        }
    }
}

#[derive(Debug, Clone, Component)]
pub struct Responser(pub Sender<CefResponse>);

#[derive(Resource, Debug, Clone, Deref)]
pub struct Requester(pub Sender<CefRequest>);

#[derive(Resource, Debug, Clone)]
pub struct RequesterReceiver(pub Receiver<CefRequest>);

#[derive(Debug, Clone)]
pub struct CefRequest {
    pub uri: String,
    pub responser: Responser,
}

/// Use to register a local schema handler for the CEF browser.
///
/// ## Reference
///
/// - [`CefSchemeHandlerFactory Class Reference`](https://cef-builds.spotifycdn.com/docs/106.1/classCefSchemeHandlerFactory.html)
pub struct LocalSchemaHandlerBuilder {
    object: *mut RcImpl<sys::_cef_scheme_handler_factory_t, Self>,
    requester: Requester,
}

impl LocalSchemaHandlerBuilder {
    pub fn build(requester: Requester) -> SchemeHandlerFactory {
        SchemeHandlerFactory::new(Self {
            object: std::ptr::null_mut(),
            requester,
        })
    }
}

impl Rc for LocalSchemaHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapSchemeHandlerFactory for LocalSchemaHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::cef_scheme_handler_factory_t, Self>) {
        self.object = object;
    }
}

impl Clone for LocalSchemaHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self {
            object,
            requester: self.requester.clone(),
        }
    }
}

impl ImplSchemeHandlerFactory for LocalSchemaHandlerBuilder {
    fn create(
        &self,
        _browser: Option<&mut Browser>,
        _frame: Option<&mut Frame>,
        _scheme_name: Option<&CefString>,
        _request: Option<&mut Request>,
    ) -> Option<ResourceHandler> {
        Some(LocalResourceHandlerBuilder::build(self.requester.clone()))
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::_cef_scheme_handler_factory_t {
        self.object.cast()
    }
}

struct LocalResourceHandlerBuilder {
    object: *mut RcImpl<_cef_resource_handler_t, Self>,
    requester: Requester,
    headers: Arc<Mutex<HeadersResponser>>,
    data: Arc<Mutex<DataResponser>>,
}

impl LocalResourceHandlerBuilder {
    fn build(requester: Requester) -> ResourceHandler {
        ResourceHandler::new(Self {
            object: std::ptr::null_mut(),
            requester,
            headers: Arc::new(Mutex::new(HeadersResponser::default())),
            data: Arc::new(Mutex::new(DataResponser::default())),
        })
    }
}

impl WrapResourceHandler for LocalResourceHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_resource_handler_t, Self>) {
        self.object = object;
    }
}

impl Clone for LocalResourceHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self {
            object,
            requester: self.requester.clone(),
            headers: self.headers.clone(),
            data: self.data.clone(),
        }
    }
}

impl Rc for LocalResourceHandlerBuilder {
    fn as_base(&self) -> &cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl ImplResourceHandler for LocalResourceHandlerBuilder {
    fn open(
        &self,
        request: Option<&mut Request>,
        handle_request: Option<&mut c_int>,
        callback: Option<&mut Callback>,
    ) -> c_int {
        let Some(request) = request else {
            // Cancel the request if no request is provided
            return 0;
        };
        let range_header_value = request.header_by_name(Some(&"Range".into())).into_string();
        let range = parse_bytes_single_range(&range_header_value);
        let Some(callback) = callback.cloned() else {
            // If no callback is provided, we cannot handle the request
            return 0;
        };
        if let Some(handle_request) = handle_request {
            *handle_request = 0;
        }
        let url = request.url().into_string();
        let uri = asset_load_path_from_request_url(&url);
        webview_debug_log(format!("scheme open url={url} uri={uri} range={range:?}"));
        let requester = self.requester.clone();
        let headers_responser = self.headers.clone();
        let data_responser = self.data.clone();
        IoTaskPool::get()
            .spawn(async move {
                let (tx, rx) = async_channel::bounded(1);
                let _ = requester
                    .send(CefRequest {
                        uri: uri.clone(),
                        responser: Responser(tx),
                    })
                    .await;
                let response = rx.recv().await.unwrap_or_default();
                webview_debug_log(format!(
                    "scheme response uri={uri} status={} mime={} len={}",
                    response.status_code,
                    response.mime_type,
                    response.data.len()
                ));
                headers_responser.lock().unwrap().prepare(&response, &range);
                data_responser
                    .lock()
                    .unwrap()
                    .prepare(response.data, &range);
                callback.cont();
            })
            .detach();
        1
    }

    fn response_headers(
        &self,
        response: Option<&mut Response>,
        response_length: Option<&mut i64>,
        _redirect_url: Option<&mut CefString>,
    ) {
        let Ok(responser) = self.headers.lock() else {
            return;
        };
        if let Some(response) = response {
            response.set_mime_type(Some(&responser.mime_type.as_str().into()));
            response.set_status(responser.status_code as _);
            for (name, value) in &responser.headers {
                response.set_header_by_name(
                    Some(&name.as_str().into()),
                    Some(&value.as_str().into()),
                    false as _,
                );
            }
        }
        if let Some(response_length) = response_length {
            *response_length = responser.response_length as _;
        }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn read(
        &self,
        data_out: *mut u8,
        bytes_to_read: c_int,
        bytes_read: Option<&mut c_int>,
        _: Option<&mut ResourceReadCallback>,
    ) -> c_int {
        let Some(bytes_read) = bytes_read else {
            // If no bytes_read is provided, we cannot read data
            return 0;
        };
        let Ok(mut responser) = self.data.lock() else {
            return 0;
        };
        match responser.read(bytes_to_read as _) {
            Some(data) if !data.is_empty() => {
                let n = data.len();
                unsafe {
                    std::ptr::copy_nonoverlapping(data.as_ptr(), data_out, n);
                }
                *bytes_read = n as i32;
                1
            }
            _ => {
                *bytes_read = 0;
                0
            }
        }
    }

    #[inline]
    fn get_raw(&self) -> *mut _cef_resource_handler_t {
        self.object.cast()
    }
}

#[cfg(test)]
mod custom_scheme_url_tests {
    use super::asset_load_path_from_request_url_with;
    use crate::util::{CefEmbeddedHost, CefEmbeddedHosts, CefEmbeddedPageConfig};

    fn test_scheme() -> &'static str {
        crate::util::compile_time_cef_embedded_scheme()
    }

    fn history_config() -> CefEmbeddedPageConfig {
        CefEmbeddedPageConfig::new(
            test_scheme(),
            CefEmbeddedHosts(vec![CefEmbeddedHost {
                host: "history".to_string(),
                default_document: "history/index.html".to_string(),
            }]),
        )
    }

    fn empty_hosts_config() -> CefEmbeddedPageConfig {
        CefEmbeddedPageConfig::new(test_scheme(), CefEmbeddedHosts::default())
    }

    #[test]
    fn registered_host_root_maps_to_default_embedded_document() {
        let cfg = history_config();
        let p = cfg.scheme_prefix();
        for url in [
            format!("{p}history/"),
            format!("{p}history"),
            format!("{p}history/?q=1"),
            format!("{p}history#frag"),
        ] {
            assert_eq!(
                asset_load_path_from_request_url_with(&url, &cfg),
                "embedded://history/index.html",
                "{url}"
            );
        }
    }

    #[test]
    fn registered_host_subpath_maps_to_embedded() {
        let cfg = history_config();
        let p = cfg.scheme_prefix();
        assert_eq!(
            asset_load_path_from_request_url_with(&format!("{p}history/other/page.html"), &cfg),
            "embedded://history/other/page.html"
        );
    }

    #[test]
    fn registered_host_subpath_normalizes_dot_segments() {
        let cfg = history_config();
        let p = cfg.scheme_prefix();
        assert_eq!(
            asset_load_path_from_request_url_with(
                &format!("{p}history/./wasm/history_app_bg.wasm"),
                &cfg
            ),
            "embedded://history/wasm/history_app_bg.wasm"
        );
    }

    #[test]
    fn custom_host_uses_its_default_document() {
        let cfg = CefEmbeddedPageConfig::new(
            test_scheme(),
            CefEmbeddedHosts(vec![CefEmbeddedHost {
                host: "help".to_string(),
                default_document: "help/index.html".to_string(),
            }]),
        );
        let p = cfg.scheme_prefix();
        assert_eq!(
            asset_load_path_from_request_url_with(&format!("{p}help/"), &cfg),
            "embedded://help/index.html"
        );
        assert_eq!(
            asset_load_path_from_request_url_with(&format!("{p}help/topic.html"), &cfg),
            "embedded://help/topic.html"
        );
    }

    #[test]
    fn cef_localhost_embedded_prefix() {
        assert_eq!(
            asset_load_path_from_request_url_with(
                "cef://localhost/embedded/crate/foo.html",
                &empty_hosts_config()
            ),
            "embedded://crate/foo.html"
        );
    }

    #[test]
    fn cef_localhost_disk_style_path() {
        assert_eq!(
            asset_load_path_from_request_url_with(
                "cef://localhost/index.html",
                &empty_hosts_config()
            ),
            "index.html"
        );
    }

    #[test]
    fn unknown_scheme_yields_empty() {
        assert_eq!(
            asset_load_path_from_request_url_with("https://example.com/", &empty_hosts_config()),
            ""
        );
    }
}
