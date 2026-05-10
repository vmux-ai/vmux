use dioxus::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlAnchorElement;

#[derive(Deserialize)]
struct Updates {
    downloads: Option<HashMap<String, DownloadEntry>>,
}

#[derive(Deserialize)]
struct DownloadEntry {
    url: String,
}

const PLATFORM_KEY: &str = "macos-aarch64";

pub fn use_is_mac() -> bool {
    use_hook(detect_is_mac)
}

pub fn use_clipboard_copy() -> Callback<String> {
    use_callback(|text: String| {
        spawn(async move {
            if let Some(window) = web_sys::window() {
                let _ = JsFuture::from(window.navigator().clipboard().write_text(&text)).await;
            }
        });
    })
}

pub fn use_dmg_download() -> Callback<()> {
    use_callback(|()| {
        spawn(async move {
            let _ = trigger_download().await;
        });
    })
}

fn detect_is_mac() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let nav = window.navigator();
    if let Ok(ua) = nav.user_agent()
        && ua.contains("Mac")
    {
        return true;
    }
    if let Ok(platform) = nav.platform()
        && platform.contains("Mac")
    {
        return true;
    }
    false
}

async fn trigger_download() -> Result<(), String> {
    let response = gloo_net::http::Request::get("/updates.json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let updates: Updates = response.json().await.map_err(|e| e.to_string())?;
    let url = updates
        .downloads
        .as_ref()
        .and_then(|d| d.get(PLATFORM_KEY))
        .map(|d| d.url.clone())
        .ok_or("no download url for platform")?;

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let body = document.body().ok_or("no body")?;
    let anchor: HtmlAnchorElement = document
        .create_element("a")
        .map_err(|_| "create_element failed")?
        .dyn_into()
        .map_err(|_| "not an anchor")?;
    anchor.set_href(&url);
    anchor.set_target("_blank");
    anchor.set_rel("noopener");
    body.append_child(&anchor).map_err(|_| "append failed")?;
    anchor.click();
    body.remove_child(&anchor).map_err(|_| "remove failed")?;
    Ok(())
}
