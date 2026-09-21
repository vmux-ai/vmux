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
}

pub trait BinEvent: 'static {
    const ID: &'static str;
    const NAMESPACE: &'static str = "";
    const NAME: &'static str;
    const VERSION: u16 = 1;
    const TARGET: BinEventTarget;

    fn id() -> &'static str {
        Self::ID
    }
}

pub trait HostEvent: BinEvent {}

pub trait UiEvent: BinEvent {}

#[derive(Clone, Copy, Debug, Default, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::component::Component))]
#[vmux_api::ui_event(namespace = "page", name = "ready", target = any)]
pub struct PageReady;

#[cfg(test)]
mod tests {
    use super::*;

    #[vmux_api::host_event(
        namespace = "test",
        name = "event",
        version = 3,
        targets = ["one", "two"]
    )]
    struct TestEvent;

    #[vmux_api::host_event(namespace = "test_event", name = "value", target = any)]
    struct UnderscoredNamespace;

    #[vmux_api::host_event(namespace = "test", name = "event_value", target = any)]
    struct UnderscoredName;

    #[test]
    fn event_id_includes_its_protocol_version() {
        assert_eq!(TestEvent::id(), "test.event@3");
    }

    #[test]
    fn event_id_components_cannot_collide() {
        assert_eq!(UnderscoredNamespace::id(), "test_event.value@1");
        assert_eq!(UnderscoredName::id(), "test.event_value@1");
        assert_ne!(UnderscoredNamespace::id(), UnderscoredName::id());
    }

    #[test]
    fn target_accepts_only_declared_hosts() {
        assert!(TestEvent::TARGET.accepts("one"));
        assert!(TestEvent::TARGET.accepts("two"));
        assert!(!TestEvent::TARGET.accepts("three"));
    }
}
