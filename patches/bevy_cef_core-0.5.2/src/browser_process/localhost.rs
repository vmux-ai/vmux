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
use std::sync::{Arc, Mutex};

use crate::util::{HOST_VMUX_HISTORY, VMUX_HISTORY_DEFAULT_DOCUMENT};

/// Map navigated custom-scheme URLs to a Bevy [`AssetServer`] load path.
///
/// - `cef://localhost/embedded/…` → `embedded://…` ([embedded assets](https://bevy.org/examples/assets/embedded-asset/)).
/// - `cef://localhost/…` (otherwise) → path as-is (disk assets under the app asset root).
/// - `vmux://history/` (host root, Chrome-style) → `embedded://<VMUX_HISTORY_DEFAULT_DOCUMENT>`.
/// - `vmux://history/<path>` → `embedded://<path>` when the remainder has no scheme.
///
/// The `vmux` scheme requires a render subprocess built from this repo’s patched `bevy_cef_core`
/// (`patches/bevy_cef_debug_render_process-0.5.2`); see the workspace `Makefile`
/// `install-debug-render-process` target.
pub(crate) fn asset_load_path_from_request_url(url: &str) -> String {
    const CEF_LOCAL: &str = concat!("cef", "://", "localhost", "/");
    const EMBEDDED_LEAF: &str = "embedded/";
    const EMBEDDED_SCHEME: &str = "embedded://";

    if let Some(rest) = url.strip_prefix(CEF_LOCAL) {
        if let Some(tail) = rest.strip_prefix(EMBEDDED_LEAF) {
            format!("{EMBEDDED_SCHEME}{tail}")
        } else {
            rest.to_string()
        }
    } else if let Some(rest) = url.strip_prefix("vmux://") {
        let path_part = rest.split(['?', '#']).next().unwrap_or(rest);
        let history_slash = format!("{HOST_VMUX_HISTORY}/");
        let tail = path_part
            .strip_prefix(&history_slash)
            .or_else(|| path_part.strip_prefix(HOST_VMUX_HISTORY))
            .unwrap_or(path_part);
        let tail = tail.trim_start_matches('/').trim_end_matches('/');
        if tail.starts_with(EMBEDDED_SCHEME) {
            tail.to_string()
        } else if tail.is_empty() {
            format!("{EMBEDDED_SCHEME}{VMUX_HISTORY_DEFAULT_DOCUMENT}")
        } else {
            format!("{EMBEDDED_SCHEME}{tail}")
        }
    } else {
        String::new()
    }
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
        let requester = self.requester.clone();
        let headers_responser = self.headers.clone();
        let data_responser = self.data.clone();
        IoTaskPool::get()
            .spawn(async move {
                let (tx, rx) = async_channel::bounded(1);
                let _ = requester
                    .send(CefRequest {
                        uri: asset_load_path_from_request_url(&url),
                        responser: Responser(tx),
                    })
                    .await;
                let response = rx.recv().await.unwrap_or_default();
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
    use super::asset_load_path_from_request_url;
    use crate::util::VMUX_HISTORY_DEFAULT_DOCUMENT;

    #[test]
    fn vmux_history_root_maps_to_default_embedded_document() {
        for url in [
            "vmux://history/",
            "vmux://history",
            "vmux://history/?q=1",
            "vmux://history#frag",
        ] {
            assert_eq!(
                asset_load_path_from_request_url(url),
                format!("embedded://{VMUX_HISTORY_DEFAULT_DOCUMENT}"),
                "{url}"
            );
        }
    }

    #[test]
    fn vmux_history_subpath_maps_to_embedded() {
        assert_eq!(
            asset_load_path_from_request_url("vmux://history/other/page.html"),
            "embedded://other/page.html"
        );
    }

    #[test]
    fn cef_localhost_embedded_prefix() {
        assert_eq!(
            asset_load_path_from_request_url("cef://localhost/embedded/crate/foo.html"),
            "embedded://crate/foo.html"
        );
    }

    #[test]
    fn cef_localhost_disk_style_path() {
        assert_eq!(
            asset_load_path_from_request_url("cef://localhost/index.html"),
            "index.html"
        );
    }

    #[test]
    fn unknown_scheme_yields_empty() {
        assert_eq!(asset_load_path_from_request_url("https://example.com/"), "");
    }
}
