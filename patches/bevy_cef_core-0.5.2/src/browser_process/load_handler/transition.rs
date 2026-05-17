#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CefTransitionCore {
    Link,
    Explicit,
    AutoBookmark,
    AutoSubframe,
    ManualSubframe,
    Generated,
    AutoToplevel,
    FormSubmit,
    Reload,
    Keyword,
    KeywordGenerated,
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CefTransitionQualifiers {
    pub forward_back: bool,
    pub from_address_bar: bool,
    pub client_redirect: bool,
    pub server_redirect: bool,
    pub chain_start: bool,
    pub chain_end: bool,
}

const SOURCE_MASK: u32 = 0xFF;
const FORWARD_BACK_FLAG: u32 = 0x01000000;
const FROM_ADDRESS_BAR_FLAG: u32 = 0x02000000;
const CHAIN_START_FLAG: u32 = 0x10000000;
const CHAIN_END_FLAG: u32 = 0x20000000;
const CLIENT_REDIRECT_FLAG: u32 = 0x40000000;
const SERVER_REDIRECT_FLAG: u32 = 0x80000000;

pub fn decode(raw: u32) -> (CefTransitionCore, CefTransitionQualifiers) {
    let core = match raw & SOURCE_MASK {
        0 => CefTransitionCore::Link,
        1 => CefTransitionCore::Explicit,
        2 => CefTransitionCore::AutoBookmark,
        3 => CefTransitionCore::AutoSubframe,
        4 => CefTransitionCore::ManualSubframe,
        5 => CefTransitionCore::Generated,
        6 => CefTransitionCore::AutoToplevel,
        7 => CefTransitionCore::FormSubmit,
        8 => CefTransitionCore::Reload,
        9 => CefTransitionCore::Keyword,
        10 => CefTransitionCore::KeywordGenerated,
        _ => CefTransitionCore::Unknown,
    };
    let qual = CefTransitionQualifiers {
        forward_back: raw & FORWARD_BACK_FLAG != 0,
        from_address_bar: raw & FROM_ADDRESS_BAR_FLAG != 0,
        client_redirect: raw & CLIENT_REDIRECT_FLAG != 0,
        server_redirect: raw & SERVER_REDIRECT_FLAG != 0,
        chain_start: raw & CHAIN_START_FLAG != 0,
        chain_end: raw & CHAIN_END_FLAG != 0,
    };
    (core, qual)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_link_no_qualifiers() {
        let (core, qual) = decode(0);
        assert_eq!(core, CefTransitionCore::Link);
        assert_eq!(qual, CefTransitionQualifiers::default());
    }

    #[test]
    fn decodes_explicit_from_address_bar() {
        let (core, qual) = decode(1 | FROM_ADDRESS_BAR_FLAG);
        assert_eq!(core, CefTransitionCore::Explicit);
        assert!(qual.from_address_bar);
        assert!(!qual.forward_back);
    }

    #[test]
    fn decodes_forward_back_link() {
        let (core, qual) = decode(0 | FORWARD_BACK_FLAG);
        assert_eq!(core, CefTransitionCore::Link);
        assert!(qual.forward_back);
    }

    #[test]
    fn decodes_server_redirect_chain() {
        let bits = 0 | SERVER_REDIRECT_FLAG | CHAIN_START_FLAG | CHAIN_END_FLAG;
        let (_, qual) = decode(bits);
        assert!(qual.server_redirect);
        assert!(qual.chain_start);
        assert!(qual.chain_end);
    }

    #[test]
    fn unknown_core_falls_through() {
        let (core, _) = decode(99);
        assert_eq!(core, CefTransitionCore::Unknown);
    }
}
