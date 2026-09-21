use std::cell::Cell;
use std::rc::Rc;

use crate::page::NativePage;
use crate::webview::dom::Dom;
use crate::webview::embed::{AssetReply, Assets};

pub(crate) struct PageRoutes {
    page: Rc<Cell<&'static NativePage>>,
    dom: Dom,
    assets: Rc<dyn Assets>,
}

impl PageRoutes {
    pub(crate) fn new(
        page: Rc<Cell<&'static NativePage>>,
        dom: Dom,
        assets: Rc<dyn Assets>,
    ) -> Self {
        Self { page, dom, assets }
    }

    pub(crate) fn serve(
        &self,
        request: wry::http::Request<Vec<u8>>,
        responder: wry::RequestAsyncResponder,
    ) {
        let url = request.uri().to_string();
        let page = self.page.get();
        let route = Route::from(url.as_str());
        if !route.is_served_by(&url, page) {
            responder.respond(
                wry::http::Response::builder()
                    .status(404)
                    .body(Vec::new())
                    .unwrap_or_else(|_| wry::http::Response::new(Vec::new())),
            );
            return;
        }
        match route {
            Route::Events => self.dom.answer_event(&request, responder),
            Route::Edits => self.dom.serve_edits(&request, responder),
            Route::Document => responder.respond(page.shell()),
            Route::Asset => self.assets.fetch(&url, AssetReply::from(responder)),
        }
    }
}

#[derive(Clone, Copy)]
enum Route {
    Events,
    Edits,
    Document,
    Asset,
}

impl Route {
    fn is_served_by(self, request: &str, page: &NativePage) -> bool {
        if Self::belongs_to(request, page.document_url()) {
            return true;
        }
        matches!(self, Self::Asset)
            && (Self::belongs_to(request, page.url) || Self::is_vmux_favicon(request))
    }

    fn is_vmux_favicon(url: &str) -> bool {
        url.starts_with("vmux://") && Self::path_of(url).starts_with("assets/favicons/")
    }

    fn belongs_to(request: &str, document: &str) -> bool {
        Self::host_of(request) == Self::host_of(document)
    }

    fn host_of(url: &str) -> &str {
        let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
        after_scheme.split('/').next().unwrap_or_default()
    }

    fn path_of(url: &str) -> &str {
        let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
        let path = after_scheme.split(['?', '#']).next().unwrap_or("");
        let after_host = path.split_once('/').map(|(_, rest)| rest).unwrap_or("");

        after_host.trim_end_matches('/')
    }
}

impl From<&str> for Route {
    fn from(url: &str) -> Self {
        match Self::path_of(url) {
            "__events" => Self::Events,
            "__edits" => Self::Edits,
            "" | "index.html" => Self::Document,
            _ => Self::Asset,
        }
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
                matches!(Route::from(url), Route::Document),
                "{url} should ask for the document"
            );
        }
    }

    #[test]
    fn the_hosts_own_routes_are_not_mistaken_for_assets() {
        assert!(matches!(
            Route::from("vmux://layout/__events"),
            Route::Events
        ));
        assert!(matches!(
            Route::from("vmux://layout/__edits/"),
            Route::Edits
        ));
    }

    #[test]
    fn a_query_or_fragment_does_not_change_the_route() {
        assert!(matches!(
            Route::from("vmux://layout/__events?v=2"),
            Route::Events
        ));
        assert!(matches!(
            Route::from("vmux://layout/?tab=1"),
            Route::Document
        ));
    }

    #[test]
    fn anything_the_document_references_is_an_asset() {
        assert!(matches!(
            Route::from("vmux://layout/assets/index.css"),
            Route::Asset
        ));
    }

    #[test]
    fn a_page_served_from_another_host_can_load_its_own_favicon() {
        let page = NativePage::pane("vmux://projects/", || unreachable!())
            .titled("Projects")
            .served_from("vmux://files/");

        assert!(Route::Asset.is_served_by("vmux://projects/assets/favicons/projects.svg", &page));
        assert!(!Route::Events.is_served_by("vmux://projects/__events", &page));
    }

    #[test]
    fn the_layout_can_load_bookmark_favicons_from_other_vmux_hosts() {
        let page = NativePage::pane("vmux://layout/", || unreachable!());

        assert!(Route::Asset.is_served_by("vmux://projects/assets/favicons/projects.svg", &page));
        assert!(Route::Asset.is_served_by("vmux://settings/assets/favicons/settings.svg", &page));
        assert!(!Route::Asset.is_served_by("vmux://projects/assets/index.css", &page));
        assert!(!Route::Events.is_served_by("vmux://projects/__events", &page));
    }

    #[test]
    fn navigation_rejects_requests_from_the_previous_document() {
        assert!(Route::belongs_to(
            "vmux://sessions/__edits",
            "vmux://sessions/"
        ));
        assert!(!Route::belongs_to(
            "vmux://start/__edits",
            "vmux://sessions/"
        ));
    }
}
