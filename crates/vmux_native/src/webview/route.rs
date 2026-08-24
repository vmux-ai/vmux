use std::rc::Rc;

use crate::page::NativePage;
use crate::webview::dom::Dom;
use crate::webview::embed::{AssetReply, Assets};

pub(crate) struct PageRoutes {
    page: &'static NativePage,
    dom: Dom,
    assets: Rc<dyn Assets>,
}

impl PageRoutes {
    pub(crate) fn new(page: &'static NativePage, dom: Dom, assets: Rc<dyn Assets>) -> Self {
        Self { page, dom, assets }
    }

    pub(crate) fn serve(
        &self,
        request: wry::http::Request<Vec<u8>>,
        responder: wry::RequestAsyncResponder,
    ) {
        let url = request.uri().to_string();
        match Route::of(&url) {
            Route::Events => self.dom.answer_event(&request, responder),
            Route::Edits => self.dom.serve_edits(&request, responder),
            Route::Document => responder.respond(self.page.shell()),
            Route::Asset => self.assets.fetch(&url, AssetReply::of(responder)),
        }
    }
}

enum Route {
    Events,
    Edits,
    Document,
    Asset,
}

impl Route {
    fn of(url: &str) -> Self {
        match Self::path_of(url) {
            "__events" => Self::Events,
            "__edits" => Self::Edits,
            "" | "index.html" => Self::Document,
            _ => Self::Asset,
        }
    }

    fn path_of(url: &str) -> &str {
        let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
        let path = after_scheme.split(['?', '#']).next().unwrap_or("");
        let after_host = path.split_once('/').map(|(_, rest)| rest).unwrap_or("");

        after_host.trim_end_matches('/')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_naming_no_path_asks_for_the_page_itself() {
        for url in [
            "vmux://layout",
            "vmux://layout/",
            "vmux://layout/index.html",
        ] {
            assert!(
                matches!(Route::of(url), Route::Document),
                "{url} should ask for the document"
            );
        }
    }

    #[test]
    fn the_hosts_own_routes_are_not_mistaken_for_assets() {
        assert!(matches!(Route::of("vmux://layout/__events"), Route::Events));
        assert!(matches!(Route::of("vmux://layout/__edits/"), Route::Edits));
    }

    #[test]
    fn a_query_or_fragment_does_not_change_the_route() {
        assert!(matches!(
            Route::of("vmux://layout/__events?v=2"),
            Route::Events
        ));
        assert!(matches!(Route::of("vmux://layout/?tab=1"), Route::Document));
    }

    #[test]
    fn anything_the_document_references_is_an_asset() {
        assert!(matches!(
            Route::of("vmux://layout/assets/index.css"),
            Route::Asset
        ));
    }
}
