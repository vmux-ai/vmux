//! What a page can ask its host for, and who answers.
//!
//! Everything the page needs arrives over `vmux://`: the document it loads, the frames it renders,
//! the verdict on an event it is blocked on, and every asset any of those reference. [`Route`] is
//! that list, so adding one is a variant rather than another branch in a chain of string tests.

use std::rc::Rc;

use crate::dom::SurfaceDom;
use crate::embed::{AssetReply, Assets};
use crate::page::NativePage;

/// One page's `vmux://` handler: what it is, what fills it, and where its assets come from.
///
/// Held by the protocol closure, so answering costs a match rather than five arguments threaded
/// through a free function.
pub(crate) struct PageRoutes {
    page: &'static NativePage,
    dom: SurfaceDom,
    assets: Rc<dyn Assets>,
}

impl PageRoutes {
    pub(crate) fn new(page: &'static NativePage, dom: SurfaceDom, assets: Rc<dyn Assets>) -> Self {
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

/// What a `vmux://` url is asking for.
enum Route {
    /// The verdict on an event, which the page is blocked on until it arrives.
    Events,
    /// The next frame, which the page holds a standing request for.
    Edits,
    /// The page itself.
    Document,
    /// Anything the document references.
    Asset,
}

impl Route {
    fn of(url: &str) -> Self {
        // The host's own routes have no file extension, so they must be recognised before anything
        // maps a path to an asset: a lookup would answer `__events` with the host's default
        // document, handing the page HTML where it expects JSON.
        match Self::path_of(url) {
            "__events" => Self::Events,
            "__edits" => Self::Edits,
            "" | "index.html" => Self::Document,
            _ => Self::Asset,
        }
    }

    /// The path a url names, with the scheme, the host, the query and any trailing slash gone.
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

    /// The document is what a url with nothing after the host asks for, however it is spelled.
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

    /// Each of these would otherwise be looked up as an asset and answered with the wrong body.
    #[test]
    fn the_hosts_own_routes_are_not_mistaken_for_assets() {
        assert!(matches!(Route::of("vmux://layout/__events"), Route::Events));
        assert!(matches!(Route::of("vmux://layout/__edits/"), Route::Edits));
    }

    /// A query is how the interpreter cache-busts, and it must not change what is being asked for.
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
