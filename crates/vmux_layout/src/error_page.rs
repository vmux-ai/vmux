#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::hooks::use_theme;
use vmux_ui::i18n::translate;

#[component]
pub fn Page() -> Element {
    use_theme();
    let params = query_params();
    let lookup = |key: &str| {
        params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    let title = lookup("title").unwrap_or_else(|| translate("error-title"));
    let message = lookup("message").unwrap_or_default();
    let url = lookup("url").unwrap_or_default();

    rsx! {
        div { class: "flex h-full min-h-0 items-center justify-center bg-background p-10 text-foreground",
            section { class: "max-w-[640px]",
                h1 { class: "mb-3 text-[28px] font-semibold leading-tight", "{title}" }
                if !message.is_empty() {
                    p { class: "text-sm leading-relaxed text-muted-foreground", "{message}" }
                }
                if !url.is_empty() {
                    code { class: "mt-4 block whitespace-pre-wrap break-words rounded-md bg-card p-3 text-sm text-foreground",
                        "{url}"
                    }
                }
            }
        }
    }
}

fn query_params() -> Vec<(String, String)> {
    let search = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();
    search
        .trim_start_matches('?')
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| {
            let mut it = pair.splitn(2, '=');
            let key = it.next()?;
            let value = it.next().unwrap_or("");
            Some((percent_decode(key), percent_decode(value)))
        })
        .collect()
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push((hi * 16 + lo) as u8);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
