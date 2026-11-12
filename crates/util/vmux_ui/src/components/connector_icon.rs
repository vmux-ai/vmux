use dioxus::prelude::*;

use crate::favicon::{Favicon, FaviconSource};

#[component]
pub fn ConnectorIcon(
    id: String,
    #[props(default = "size-5 shrink-0".to_string())] class: String,
) -> Element {
    rsx! {
        Favicon {
            favicon_url: String::new(),
            url: FaviconSource::connector(&id).unwrap_or_default().to_string(),
            class: Some(class.clone()),
            globe_class: Some(format!("{class} text-muted-foreground")),
        }
    }
}
