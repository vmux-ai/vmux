//! CEF custom URL schemes used with Bevy assets.
//!
//! Chromium requires **two** registrations (see [CEF scheme registration](https://cef-builds.spotifycdn.com/docs/145.0/classCefApp.html)):
//!
//! 1. **`on_register_custom_schemes`** — in **every** process (browser + render). Implemented on
//!    [`BrowserProcessAppBuilder`](crate::browser_process::app::BrowserProcessAppBuilder) and
//!    [`RenderProcessAppBuilder`](crate::render_process::app::RenderProcessAppBuilder) via
//!    [`cef_scheme_flags`].
//! 2. **Scheme handler factories** — per [`RequestContext`](cef::RequestContext) *and* process-wide via
//!    [`cef::register_scheme_handler_factory`](cef::register_scheme_handler_factory) (see
//!    [`Browsers::create_browser`](crate::browser_process::browsers::Browsers::create_browser)) so
//!    the custom embedded page scheme ([`CefEmbeddedPageConfig`]) is recognized everywhere Chromium resolves URLs.
//!
//! On macOS debug builds, install the workspace `bevy_cef_debug_render_process` **inside** the CEF
//! framework’s `Libraries/` directory (`make install-debug-render-process`). Chromium resolves GPU
//! and other dylibs relative to the helper executable; placing the helper only in `target/debug/`
//! breaks that lookup and can surface as `ERR_UNKNOWN_URL_SCHEME` for custom schemes.

pub mod v8_accessor;
mod v8_handler_wrapper;
pub mod v8_interceptor;

use crate::util::v8_accessor::V8DefaultAccessorBuilder;
use crate::util::v8_interceptor::V8DefaultInterceptorBuilder;
use cef::rc::ConvertParam;
use cef::{
    CefStringList, CefStringUserfreeUtf16, CefStringUtf16, ImplV8Value, V8Propertyattribute,
    v8_value_create_array, v8_value_create_bool, v8_value_create_double, v8_value_create_int,
    v8_value_create_null, v8_value_create_object, v8_value_create_string,
};
use cef_dll_sys::_cef_string_utf16_t;
use cef_dll_sys::cef_scheme_options_t::{
    CEF_SCHEME_OPTION_CORS_ENABLED, CEF_SCHEME_OPTION_FETCH_ENABLED, CEF_SCHEME_OPTION_LOCAL,
    CEF_SCHEME_OPTION_SECURE, CEF_SCHEME_OPTION_STANDARD,
};
use std::env::home_dir;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const EXTENSIONS_SWITCH: &str = "bevy-cef-extensions";

pub const SCHEME_CEF: &str = "cef";

pub const FILES_SCHEME: &str = "file";

pub const HOST_CEF: &str = "localhost";

pub fn compile_time_cef_embedded_scheme() -> &'static str {
    include_str!(concat!(env!("OUT_DIR"), "/cef_embedded_scheme.txt")).trim()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CefEmbeddedHost {
    pub host: String,
    pub default_document: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CefEmbeddedHosts(pub Vec<CefEmbeddedHost>);

impl CefEmbeddedHosts {
    pub fn entry_for_host(&self, host: &str) -> Option<&CefEmbeddedHost> {
        self.0.iter().find(|e| e.host == host)
    }
}

impl From<Vec<CefEmbeddedHost>> for CefEmbeddedHosts {
    fn from(entries: Vec<CefEmbeddedHost>) -> Self {
        Self(entries)
    }
}

impl Default for CefEmbeddedHosts {
    fn default() -> Self {
        Self(Vec::new())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CefEmbeddedPageConfig {
    pub scheme: String,
    scheme_prefix: String,
    pub hosts: CefEmbeddedHosts,
}

impl CefEmbeddedPageConfig {
    pub fn new(scheme: impl Into<String>, hosts: CefEmbeddedHosts) -> Self {
        let scheme = scheme.into().trim().to_string();
        let scheme_prefix = format!("{scheme}://");
        Self {
            scheme,
            scheme_prefix,
            hosts,
        }
    }

    pub fn scheme_prefix(&self) -> &str {
        &self.scheme_prefix
    }
}

impl Default for CefEmbeddedPageConfig {
    fn default() -> Self {
        Self::new(
            compile_time_cef_embedded_scheme(),
            CefEmbeddedHosts::default(),
        )
    }
}

static CEF_EMBEDDED_PAGE_OVERRIDE: OnceLock<Arc<CefEmbeddedPageConfig>> = OnceLock::new();

pub fn try_set_cef_embedded_page_config(config: CefEmbeddedPageConfig) {
    let _ = CEF_EMBEDDED_PAGE_OVERRIDE.set(Arc::new(config));
}

pub fn resolved_cef_embedded_page_config() -> Arc<CefEmbeddedPageConfig> {
    CEF_EMBEDDED_PAGE_OVERRIDE
        .get()
        .cloned()
        .unwrap_or_else(|| Arc::new(CefEmbeddedPageConfig::default()))
}

pub fn url_has_embedded_scheme(url: &str, scheme_prefix: &str) -> bool {
    !scheme_prefix.is_empty() && url.starts_with(scheme_prefix)
}

pub fn embedded_page_host(url: &str, scheme_prefix: &str) -> Option<String> {
    if scheme_prefix.is_empty() {
        return None;
    }
    let rest = url.strip_prefix(scheme_prefix)?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

pub fn url_is_trusted_embedded_page(
    url: &str,
    scheme_prefix: &str,
    hosts: &CefEmbeddedHosts,
) -> bool {
    match embedded_page_host(url, scheme_prefix) {
        Some(host) => hosts.entry_for_host(&host).is_some(),
        None => false,
    }
}

pub fn has_embedded_scheme(url: &str) -> bool {
    url_has_embedded_scheme(url, resolved_cef_embedded_page_config().scheme_prefix())
        || url.starts_with("file://")
}

pub fn is_trusted_embedded_page(url: &str) -> bool {
    if url.starts_with("file://") {
        return true;
    }
    let config = resolved_cef_embedded_page_config();
    url_is_trusted_embedded_page(url, config.scheme_prefix(), &config.hosts)
}

pub const BRIDGE_ALLOWED_AUTHORITIES: &[&str] = &[
    "chat.mistral.ai",
    "chat.local.mistral.ai:8443",
    "chromewebstore.google.com",
];

pub fn is_bridge_allowed_origin(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    BRIDGE_ALLOWED_AUTHORITIES.contains(&authority)
}

pub fn ipc_allowed_render(url: &str) -> bool {
    has_embedded_scheme(url) || is_bridge_allowed_origin(url)
}

pub fn ipc_allowed_browser(url: &str) -> bool {
    is_trusted_embedded_page(url) || is_bridge_allowed_origin(url)
}

pub fn embedded_page_host_of(url: &str) -> Option<String> {
    embedded_page_host(url, resolved_cef_embedded_page_config().scheme_prefix())
}

pub fn webview_debug_log_enabled() -> bool {
    std::env::var_os("VMUX_WEBVIEW_DEBUG").is_some()
}

fn webview_debug_log_path() -> PathBuf {
    std::env::var_os("VMUX_WEBVIEW_DEBUG_LOG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/vmux_webview_debug.log"))
}

pub fn reset_webview_debug_log() {
    if !webview_debug_log_enabled() {
        return;
    }
    let _ = std::fs::remove_file(webview_debug_log_path());
}

pub fn webview_debug_log(message: impl AsRef<str>) {
    if !webview_debug_log_enabled() {
        return;
    }
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(webview_debug_log_path())
    {
        let _ = writeln!(
            file,
            "{ts_ms} pid={} {}",
            std::process::id(),
            message.as_ref()
        );
    }
}

pub fn cef_scheme_flags() -> u32 {
    CEF_SCHEME_OPTION_STANDARD as u32
        | CEF_SCHEME_OPTION_SECURE as u32
        | CEF_SCHEME_OPTION_LOCAL as u32
        | CEF_SCHEME_OPTION_CORS_ENABLED as u32
        | CEF_SCHEME_OPTION_FETCH_ENABLED as u32
}

static MEDIA_ALLOWLIST: std::sync::LazyLock<
    std::sync::RwLock<std::collections::HashSet<PathBuf>>,
> = std::sync::LazyLock::new(|| std::sync::RwLock::new(std::collections::HashSet::new()));

/// Replace the set of absolute paths the raw-media resource handler is allowed to
/// read. Paths are canonicalized; callers pass the live set each frame.
pub fn set_media_allowlist(paths: std::collections::HashSet<PathBuf>) {
    let canon: std::collections::HashSet<PathBuf> = paths
        .into_iter()
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .collect();
    if let Ok(mut w) = MEDIA_ALLOWLIST.write() {
        *w = canon;
    }
}

/// Whether `path` is currently allowed to be served as raw media.
pub fn is_media_path_allowed(path: &std::path::Path) -> bool {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    MEDIA_ALLOWLIST
        .read()
        .map(|s| s.contains(&canon))
        .unwrap_or(false)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_decode_path(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(a), Some(b)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            out.push(a * 16 + b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// If `url` is a `file://` URL carrying the `vmux-raw=1` marker, return its
/// decoded absolute path. Otherwise `None` (normal document/asset navigation).
pub fn raw_media_request(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let (path_part, query) = rest.split_once('?')?;
    if !query.split('&').any(|kv| kv == "vmux-raw=1") {
        return None;
    }
    let path = PathBuf::from(percent_decode_path(path_part));
    path.is_absolute().then_some(path)
}

/// MIME type for raw-media serving (kept local to avoid a patch→vmux_core dep).
pub fn raw_media_mime(path: &std::path::Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" | "mov" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" | "opus" => "audio/ogg",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// Resolved raw-media response: HTTP status, the bytes to stream (already sliced
/// to the requested range), the response headers, and the MIME type.
pub struct RawMediaResponse {
    pub status: u32,
    pub headers: Vec<(String, String)>,
    pub mime: String,
    /// File handle seeked to the response start, to be streamed on demand. `None`
    /// for error responses (404/416) and empty files.
    pub file: Option<std::fs::File>,
    /// Number of bytes to stream from `file`.
    pub len: usize,
}

fn cors_headers() -> Vec<(String, String)> {
    vec![
        ("Access-Control-Allow-Origin".to_string(), "*".to_string()),
        ("Access-Control-Allow-Methods".to_string(), "*".to_string()),
        ("Access-Control-Allow-Headers".to_string(), "*".to_string()),
    ]
}

fn deny(status: u32, mime: &str) -> RawMediaResponse {
    RawMediaResponse {
        status,
        headers: cors_headers(),
        mime: mime.to_string(),
        file: None,
        len: 0,
    }
}

/// Resolve an allowlisted media file and return a file handle seeked to the start
/// of the requested byte range, to be streamed on demand. `range` is the parsed
/// single byte-range `(start, end_inclusive?)` with HTTP-inclusive end semantics.
/// The full requested range is served (no chunk cap) so the player can seek to an
/// end-located moov atom; memory stays bounded because bytes are read lazily.
/// Returns 404 if the path is not allowlisted or unreadable, 416 if the range
/// start is past the end of the file.
pub fn build_raw_media_response(
    path: &std::path::Path,
    range: &Option<(usize, Option<usize>)>,
) -> RawMediaResponse {
    use std::io::{Seek, SeekFrom};

    // Canonicalize once and use the result for both the allowlist check and the
    // open, so a symlink swap between them can't redirect to a non-allowlisted file.
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let path = canonical.as_path();

    let allowed = is_media_path_allowed(path);
    raw_media_debug(&format!(
        "req path={} allowed={allowed} range={range:?}",
        path.display()
    ));
    if !allowed {
        return deny(404, "text/plain");
    }
    let mime = raw_media_mime(path).to_string();
    let (mut file, total) = match std::fs::File::open(path)
        .and_then(|f| Ok((f.metadata()?.len() as usize, f)))
    {
        Ok((len, f)) => (f, len),
        Err(_) => return deny(404, "text/plain"),
    };

    let mut headers = cors_headers();
    headers.push(("Accept-Ranges".to_string(), "bytes".to_string()));

    if total == 0 {
        return RawMediaResponse {
            status: 200,
            headers,
            mime,
            file: None,
            len: 0,
        };
    }

    let (start, last, partial) = match range {
        Some((s, end_opt)) => {
            let s = *s;
            if s >= total {
                headers.push(("Content-Range".to_string(), format!("bytes */{total}")));
                return RawMediaResponse {
                    status: 416,
                    headers,
                    mime,
                    file: None,
                    len: 0,
                };
            }
            (
                s,
                end_opt.map(|e| e.min(total - 1)).unwrap_or(total - 1).max(s),
                true,
            )
        }
        None => (0, total - 1, false),
    };
    let status = if partial { 206 } else { 200 };
    if partial {
        headers.push((
            "Content-Range".to_string(),
            format!("bytes {start}-{last}/{total}"),
        ));
    }

    if file.seek(SeekFrom::Start(start as u64)).is_err() {
        return deny(404, &mime);
    }
    let len = last - start + 1;
    raw_media_debug(&format!(
        "serve path={} total={total} start={start} last={last} status={status} len={len}",
        path.display()
    ));

    RawMediaResponse {
        status,
        headers,
        mime,
        file: Some(file),
        len,
    }
}

fn raw_media_debug(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/vmux_media_debug.log")
    {
        let _ = writeln!(f, "{msg}");
    }
}

#[cfg(test)]
mod media_raw_tests {
    use super::*;

    #[test]
    fn raw_marker_detected_and_decoded() {
        assert_eq!(
            raw_media_request("file:///a/b/Screenshot%20x.png?vmux-raw=1"),
            Some(PathBuf::from("/a/b/Screenshot x.png"))
        );
        assert_eq!(
            raw_media_request("file:///a/b/c.mp4?foo=1&vmux-raw=1"),
            Some(PathBuf::from("/a/b/c.mp4"))
        );
    }

    #[test]
    fn non_raw_returns_none() {
        assert_eq!(raw_media_request("file:///a/b/c.png"), None);
        assert_eq!(raw_media_request("file:///a/b/c.png?other=1"), None);
        assert_eq!(raw_media_request("vmux://files/index.html?vmux-raw=1"), None);
    }

    #[test]
    fn mime_lookup() {
        assert_eq!(raw_media_mime(std::path::Path::new("a.mp4")), "video/mp4");
        assert_eq!(raw_media_mime(std::path::Path::new("a.png")), "image/png");
        assert_eq!(
            raw_media_mime(std::path::Path::new("a.bin")),
            "application/octet-stream"
        );
    }
}

#[cfg(test)]
mod raw_response_tests {
    use super::*;
    use std::collections::HashSet;

    // These tests mutate the process-global allowlist; serialize them.
    static ALLOWLIST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn denied_when_not_allowlisted() {
        let _g = ALLOWLIST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_media_allowlist(HashSet::new());
        let r = build_raw_media_response(std::path::Path::new("/no/such/file.png"), &None);
        assert_eq!(r.status, 404);
        assert!(r.file.is_none());
    }

    #[test]
    fn serves_inclusive_range_with_content_range_header() {
        let _g = ALLOWLIST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join("vmux_raw_test");
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("blob.bin");
        std::fs::write(&p, (0u8..100).collect::<Vec<u8>>()).unwrap();
        let canon = std::fs::canonicalize(&p).unwrap();

        let mut set = HashSet::new();
        set.insert(canon.clone());
        set_media_allowlist(set);

        let r = build_raw_media_response(&canon, &Some((10, Some(20))));
        assert_eq!(r.status, 206);
        assert_eq!(r.len, 11);
        let mut buf = vec![0u8; r.len];
        std::io::Read::read_exact(&mut r.file.expect("file"), &mut buf).unwrap();
        assert_eq!(buf, (10u8..=20).collect::<Vec<u8>>());
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| k == "Content-Range" && v == "bytes 10-20/100")
        );

        set_media_allowlist(HashSet::new());
        let _ = std::fs::remove_file(&p);
    }
}

pub fn debug_chromium_libraries_path() -> PathBuf {
    debug_chromium_embedded_framework_dir_path().join("Libraries")
}

pub fn debug_chromium_embedded_framework_dir_path() -> PathBuf {
    home_dir()
        .unwrap()
        .join(".local")
        .join("share")
        .join("Chromium Embedded Framework.framework")
}

pub fn debug_render_process_path() -> PathBuf {
    debug_chromium_libraries_path().join("bevy_cef_debug_render_process")
}

/// Returns the path to the render process binary next to the current executable.
///
/// On Windows: `<exe_dir>/bevy_cef_render_process.exe`
/// On macOS (release): `<exe_dir>/bevy_cef_render_process`
pub fn render_process_path() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    #[cfg(target_os = "windows")]
    let binary_name = "bevy_cef_render_process.exe";
    #[cfg(not(target_os = "windows"))]
    let binary_name = "bevy_cef_render_process";
    let path = exe_dir.join(binary_name);
    if path.exists() { Some(path) } else { None }
}

pub trait IntoString {
    fn into_string(self) -> String;
}

impl IntoString for CefStringUserfreeUtf16 {
    fn into_string(self) -> String {
        let ptr: *mut _cef_string_utf16_t = self.into_raw();
        CefStringUtf16::from(ptr).to_string()
    }
}

pub fn v8_value_to_json(v8: &cef::V8Value) -> Option<serde_json::Value> {
    if v8.is_bool().is_positive() {
        Some(serde_json::Value::Bool(v8.bool_value().is_positive()))
    } else if v8.is_int().is_positive() {
        Some(serde_json::Value::Number(serde_json::Number::from(
            v8.int_value(),
        )))
    } else if v8.is_double().is_positive() {
        Some(serde_json::Value::Number(
            serde_json::Number::from_f64(v8.double_value()).unwrap(),
        ))
    } else if v8.is_string().is_positive() {
        Some(serde_json::Value::String(v8.string_value().into_string()))
    } else if v8.is_null().is_positive() || v8.is_undefined().is_positive() {
        Some(serde_json::Value::Null)
    } else if v8.is_array().is_positive() {
        let mut array = Vec::new();
        let mut keys = CefStringList::new();
        v8.keys(Some(&mut keys));
        for key in keys.into_iter() {
            if let Some(v) = v8.value_bykey(Some(&key.as_str().into()))
                && let Some(serialized) = v8_value_to_json(&v)
            {
                {
                    array.push(serialized);
                }
            }
        }
        Some(serde_json::Value::Array(array))
    } else if v8.is_object().is_positive() {
        let mut object = serde_json::Map::new();
        let mut keys = CefStringList::new();
        v8.keys(Some(&mut keys));
        for key in keys.into_iter() {
            if let Some(v) = v8.value_bykey(Some(&key.as_str().into()))
                && let Some(serialized) = v8_value_to_json(&v)
            {
                {
                    object.insert(key, serialized);
                }
            }
        }
        Some(serde_json::Value::Object(object))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
        old_log: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn clear_debug_log_path() -> Self {
            let guard = ENV_LOCK.lock().expect("env lock");
            let old_log = std::env::var_os("VMUX_WEBVIEW_DEBUG_LOG");
            unsafe {
                std::env::remove_var("VMUX_WEBVIEW_DEBUG_LOG");
            }
            Self {
                _guard: guard,
                old_log,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(old_log) = &self.old_log {
                    std::env::set_var("VMUX_WEBVIEW_DEBUG_LOG", old_log);
                } else {
                    std::env::remove_var("VMUX_WEBVIEW_DEBUG_LOG");
                }
            }
        }
    }

    #[test]
    fn default_webview_debug_log_path_is_stable_tmp_path() {
        let _env = EnvGuard::clear_debug_log_path();

        assert_eq!(
            webview_debug_log_path(),
            PathBuf::from("/tmp/vmux_webview_debug.log")
        );
    }

    fn allowlisted_hosts() -> CefEmbeddedHosts {
        CefEmbeddedHosts(vec![
            CefEmbeddedHost {
                host: "history".to_string(),
                default_document: "index.html".to_string(),
            },
            CefEmbeddedHost {
                host: "terminal".to_string(),
                default_document: "index.html".to_string(),
            },
        ])
    }

    #[test]
    fn embedded_scheme_matches_only_exact_scheme_prefix() {
        assert!(url_has_embedded_scheme("vmux://history/", "vmux://"));
        assert!(url_has_embedded_scheme("vmux://anything/at/all", "vmux://"));
        assert!(!url_has_embedded_scheme("https://evil.com/", "vmux://"));
        assert!(!url_has_embedded_scheme("about:blank", "vmux://"));
        assert!(!url_has_embedded_scheme("vmux:evil", "vmux://"));
        assert!(!url_has_embedded_scheme("", "vmux://"));
    }

    #[test]
    fn embedded_scheme_rejects_empty_prefix() {
        assert!(!url_has_embedded_scheme("vmux://history/", ""));
        assert!(!url_has_embedded_scheme("anything", ""));
    }

    #[test]
    fn trusted_page_requires_scheme_and_allowlisted_host() {
        let hosts = allowlisted_hosts();
        assert!(url_is_trusted_embedded_page(
            "vmux://history/",
            "vmux://",
            &hosts
        ));
        assert!(url_is_trusted_embedded_page(
            "vmux://history/sub/path?x=1",
            "vmux://",
            &hosts
        ));
        assert!(url_is_trusted_embedded_page(
            "vmux://history?x=1",
            "vmux://",
            &hosts
        ));
        assert!(url_is_trusted_embedded_page(
            "vmux://terminal/",
            "vmux://",
            &hosts
        ));
    }

    #[test]
    fn trusted_page_rejects_unknown_host_and_untrusted_origins() {
        let hosts = allowlisted_hosts();
        assert!(!url_is_trusted_embedded_page(
            "vmux://unknown/",
            "vmux://",
            &hosts
        ));
        assert!(!url_is_trusted_embedded_page("vmux://", "vmux://", &hosts));
        assert!(!url_is_trusted_embedded_page(
            "vmux:evil",
            "vmux://",
            &hosts
        ));
        assert!(!url_is_trusted_embedded_page(
            "https://evil.com/",
            "vmux://",
            &hosts
        ));
        assert!(!url_is_trusted_embedded_page(
            "about:blank",
            "vmux://",
            &hosts
        ));
        assert!(!url_is_trusted_embedded_page("", "vmux://", &hosts));
        assert!(!url_is_trusted_embedded_page("vmux://history/", "", &hosts));
    }

    #[test]
    fn embedded_page_host_parses_host_segment() {
        assert_eq!(
            embedded_page_host("vmux://history/", "vmux://").as_deref(),
            Some("history")
        );
        assert_eq!(
            embedded_page_host("vmux://agent/vibe/setup", "vmux://").as_deref(),
            Some("agent")
        );
        assert_eq!(
            embedded_page_host("vmux://command-bar?x=1", "vmux://").as_deref(),
            Some("command-bar")
        );
        assert_eq!(embedded_page_host("vmux://", "vmux://"), None);
        assert_eq!(embedded_page_host("https://evil.com/", "vmux://"), None);
        assert_eq!(embedded_page_host("vmux://history/", ""), None);
    }
}

pub fn json_to_v8(v: serde_json::Value) -> Option<cef::V8Value> {
    match v {
        serde_json::Value::Null => v8_value_create_null(),
        serde_json::Value::Bool(b) => v8_value_create_bool(b as _),
        serde_json::Value::Number(n) if n.is_i64() => v8_value_create_int(n.as_i64()? as i32),
        serde_json::Value::Number(n) => v8_value_create_double(n.as_f64()?),
        serde_json::Value::String(s) => v8_value_create_string(Some(&s.as_str().into())),
        serde_json::Value::Array(arr) => {
            let v8_array = v8_value_create_array(arr.len() as _)?;
            for (i, item) in arr.into_iter().enumerate() {
                v8_array.set_value_byindex(i as _, json_to_v8(item).as_mut());
            }
            Some(v8_array)
        }
        serde_json::Value::Object(obj) => {
            let v8_object = v8_value_create_object(
                Some(&mut V8DefaultAccessorBuilder::build()),
                Some(&mut V8DefaultInterceptorBuilder::build()),
            )?;
            for (key, value) in obj {
                v8_object.set_value_bykey(
                    Some(&key.as_str().into()),
                    json_to_v8(value).as_mut(),
                    V8Propertyattribute::default(),
                );
            }
            Some(v8_object)
        }
    }
}
