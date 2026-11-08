use std::cell::Cell;
use std::rc::Rc;

use crate::page::Page;
use crate::webview::embed::{AssetReply, Assets};
use crate::webview::{Appearance, dom::Dom};

pub(crate) struct PageRoutes {
    page: Rc<Cell<&'static Page>>,
    appearance: Rc<Cell<Appearance>>,
    dom: Dom,
    assets: Rc<dyn Assets>,
}

impl PageRoutes {
    pub(crate) fn serves_document(url: &str, page: &Page) -> bool {
        Route::belongs_to(url, page.document_url())
            && Route::path_of(url) == Route::path_of(page.document_url())
    }

    pub(crate) fn new(
        page: Rc<Cell<&'static Page>>,
        appearance: Rc<Cell<Appearance>>,
        dom: Dom,
        assets: Rc<dyn Assets>,
    ) -> Self {
        Self {
            page,
            appearance,
            dom,
            assets,
        }
    }

    pub(crate) fn serve(
        &self,
        request: wry::http::Request<Vec<u8>>,
        responder: wry::RequestAsyncResponder,
    ) {
        let url = request.uri().to_string();
        let page = self.page.get();
        let route = Route::for_page(&url, page);
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
            Route::Document => responder.respond(page.shell(self.appearance.get())),
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
    fn for_page(url: &str, page: &Page) -> Self {
        if PageRoutes::serves_document(url, page) {
            return Self::Document;
        }
        let route = Self::from(url);
        let path = Self::path_of(url);
        if page.owns_subtree
            && Self::belongs_to(url, page.url)
            && !matches!(route, Self::Events | Self::Edits)
            && !path.starts_with("assets/")
            && !path.starts_with("__")
        {
            return Self::Document;
        }
        route
    }

    fn is_served_by(self, request: &str, page: &Page) -> bool {
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
        request.split_once(':').map(|(scheme, _)| scheme)
            == document.split_once(':').map(|(scheme, _)| scheme)
            && Self::host_of(request) == Self::host_of(document)
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

    fn test_page() -> Page {
        Page::pane("vmux://layout/", || unreachable!())
    }

    #[test]
    fn ipc_source_must_be_the_current_vmux_document() {
        let page = test_page();

        assert!(PageRoutes::serves_document("vmux://layout/", &page));
        assert!(PageRoutes::serves_document(
            "vmux://layout/?view=board#active",
            &page
        ));
        assert!(!PageRoutes::serves_document("vmux://layout/other", &page));
        assert!(!PageRoutes::serves_document("https://example.com/", &page));
    }

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
    fn a_nested_page_url_asks_for_its_document() {
        let page = Page::pane("vmux://tools/lsp", || unreachable!());

        assert!(matches!(
            Route::for_page("vmux://tools/lsp", &page),
            Route::Document
        ));
        assert!(matches!(
            Route::for_page("vmux://tools/lsp/asset.svg", &page),
            Route::Asset
        ));
    }

    #[test]
    fn a_subtree_url_asks_for_the_owning_page_document() {
        let page = Page::pane("vmux://projects/", || unreachable!()).owning_subtree();

        assert!(matches!(
            Route::for_page("vmux://projects/github.com/mistralai/dashboard", &page),
            Route::Document
        ));
        assert!(matches!(
            Route::for_page("vmux://projects/assets/index.css", &page),
            Route::Asset
        ));
    }

    #[test]
    fn a_subtree_page_keeps_reserved_endpoints_as_assets() {
        let page = Page::pane("vmux://simulator/", || unreachable!()).owning_subtree();

        assert!(matches!(
            Route::for_page(
                "vmux://simulator/__simulator-frame?port=58352&after=42",
                &page
            ),
            Route::Asset
        ));
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
        let page = Page::pane("vmux://projects/", || unreachable!())
            .titled("Projects")
            .served_from("vmux://files/");

        assert!(Route::Asset.is_served_by("vmux://projects/assets/favicons/projects.svg", &page));
        assert!(!Route::Events.is_served_by("vmux://projects/__events", &page));
    }

    #[test]
    fn the_layout_can_load_bookmark_favicons_from_other_vmux_hosts() {
        let page = Page::pane("vmux://layout/", || unreachable!());

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

    #[test]
    fn git_documents_assets_and_event_routes_share_the_git_origin() {
        let page = Page::pane("git://", || unreachable!()).served_from("git://git/");
        assert!(PageRoutes::serves_document("git://git/", &page));
        assert!(matches!(
            Route::for_page("git://git/assets/index.css", &page),
            Route::Asset
        ));
        assert!(Route::Events.is_served_by("git://git/__events", &page));
        assert!(Route::Edits.is_served_by("git://git/__edits", &page));
        assert!(!Route::Events.is_served_by("vmux://git/__events", &page));
    }
}
