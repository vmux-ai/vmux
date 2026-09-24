#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinEventTarget {
    Any,
    Host(&'static str),
    Hosts(&'static [&'static str]),
}

impl BinEventTarget {
    pub fn accepts(self, host: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Host(target) => target == host,
            Self::Hosts(targets) => targets.contains(&host),
        }
    }

    pub fn accepts_any(self, hosts: &[&str]) -> bool {
        hosts.iter().any(|host| self.accepts(host))
    }
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

#[derive(Clone, Copy, Debug, Default, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::component::Component))]
#[derive(vmux_api::UiEvent)]
#[event(target = any)]
pub struct PageReady;

#[cfg(test)]
mod tests {
    use super::*;

    enum Events {}

    impl BinEventFamily for Events {
        const TARGET: BinEventTarget = BinEventTarget::Host("layout");
    }

    #[derive(vmux_api::UiEvent)]
    struct DerivedOpenRequest;

    #[derive(vmux_api::UiEvent)]
    #[event(version = 2, targets = ["one", "two"])]
    struct ExplicitOpenRequest;

    #[vmux_api::host_event(
        version = 3,
        targets = ["one", "two"]
    )]
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
    fn target_accepts_only_declared_hosts() {
        assert!(TestEvent::TARGET.accepts("one"));
        assert!(TestEvent::TARGET.accepts("two"));
        assert!(!TestEvent::TARGET.accepts("three"));
        assert!(TestEvent::TARGET.accepts_any(&["zero", "two"]));
        assert!(!TestEvent::TARGET.accepts_any(&["zero", "three"]));
    }

    #[test]
    fn derive_infers_event_contract_from_type_and_family() {
        assert_eq!(DerivedOpenRequest::id(), "derived_open@1");
        assert_eq!(DerivedOpenRequest::NAME, "derived_open");
        assert_eq!(DerivedOpenRequest::TARGET, BinEventTarget::Host("layout"));
    }

    #[test]
    fn derive_accepts_explicit_event_contract() {
        assert_eq!(ExplicitOpenRequest::id(), "explicit_open@2");
        assert_eq!(ExplicitOpenRequest::NAME, "explicit_open");
        assert_eq!(
            ExplicitOpenRequest::TARGET,
            BinEventTarget::Hosts(&["one", "two"])
        );
    }
}
