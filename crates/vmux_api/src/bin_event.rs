#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinEventTarget {
    Any,
    Url(&'static str),
    Urls(&'static [&'static str]),
}

impl BinEventTarget {
    pub fn accepts(self, page_url: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Url(target) => page_url_matches(target, page_url),
            Self::Urls(targets) => targets
                .iter()
                .any(|target| page_url_matches(target, page_url)),
        }
    }

    pub fn accepts_any(self, page_urls: &[&str]) -> bool {
        page_urls.iter().any(|page_url| self.accepts(page_url))
    }
}

fn page_url_matches(target: &str, page_url: &str) -> bool {
    if target.ends_with("://") {
        return page_url.starts_with(target);
    }
    let (Ok(target), Ok(page)) = (url::Url::parse(target), url::Url::parse(page_url)) else {
        return false;
    };
    if target.scheme() != page.scheme() || target.host_str() != page.host_str() {
        return false;
    }
    let target_path = target.path().trim_end_matches('/');
    let page_path = page.path().trim_end_matches('/');
    target_path.is_empty()
        || page_path == target_path
        || page_path
            .strip_prefix(target_path)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

pub trait BinEvent: 'static {
    const ID: &'static str;
    const NAME: &'static str;
    const VERSION: u16 = 1;
    const TARGET: BinEventTarget;

    fn id() -> &'static str {
        Self::ID
    }
}

pub trait BinEventFamily {
    const TARGET: BinEventTarget;
}

pub trait HostEvent: BinEvent {}

pub trait UiEvent: BinEvent {}

#[vmux_api::ui_event(Copy, Default, target = any)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::component::Component))]
pub struct PageReady;

#[cfg(test)]
mod tests {
    use super::*;

    enum Events {}

    impl BinEventFamily for Events {
        const TARGET: BinEventTarget = BinEventTarget::Url("vmux://layout/");
    }

    #[vmux_api::ui_event]
    struct DerivedOpenRequest;

    #[vmux_api::ui_event(version = 2, urls = ["vmux://one/", "vmux://two/"])]
    struct ExplicitOpenRequest;

    #[vmux_api::host_event(version = 3, urls = ["vmux://one/", "vmux://two/"])]
    struct TestEvent;

    #[vmux_api::host_event(target = any)]
    struct FirstEvent;

    #[vmux_api::host_event(target = any)]
    struct SecondEvent;

    #[test]
    fn event_id_includes_its_protocol_version() {
        assert_eq!(TestEvent::id(), "test@3");
    }

    #[test]
    fn complete_event_names_do_not_collide() {
        assert_eq!(FirstEvent::id(), "first@1");
        assert_eq!(SecondEvent::id(), "second@1");
        assert_ne!(FirstEvent::id(), SecondEvent::id());
    }

    #[test]
    fn target_accepts_only_declared_page_urls() {
        assert!(TestEvent::TARGET.accepts("vmux://one/"));
        assert!(TestEvent::TARGET.accepts("vmux://two/child"));
        assert!(!TestEvent::TARGET.accepts("vmux://three/"));
        assert!(TestEvent::TARGET.accepts_any(&["vmux://zero/", "vmux://two/child"]));
        assert!(!TestEvent::TARGET.accepts_any(&["vmux://zero/", "vmux://three/"]));
    }

    #[test]
    fn derive_infers_event_contract_from_type_and_family() {
        assert_eq!(DerivedOpenRequest::id(), "derived_open@1");
        assert_eq!(DerivedOpenRequest::NAME, "derived_open");
        assert_eq!(
            DerivedOpenRequest::TARGET,
            BinEventTarget::Url("vmux://layout/")
        );
    }

    #[test]
    fn derive_accepts_explicit_event_contract() {
        assert_eq!(ExplicitOpenRequest::id(), "explicit_open@2");
        assert_eq!(ExplicitOpenRequest::NAME, "explicit_open");
        assert_eq!(
            ExplicitOpenRequest::TARGET,
            BinEventTarget::Urls(&["vmux://one/", "vmux://two/"])
        );
    }

    #[test]
    fn scheme_page_targets_accept_their_whole_document_space() {
        assert!(BinEventTarget::Url("file://").accepts("file:///tmp/main.rs"));
        assert!(BinEventTarget::Url("git://").accepts("git://repo/status"));
        assert!(!BinEventTarget::Url("git://").accepts("file:///tmp/main.rs"));
    }
}
