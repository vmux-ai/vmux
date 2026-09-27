pub trait BinEvent: 'static {
    const ID: &'static str;
    const NAME: &'static str;
    const PERMISSION: &'static str;
    const VERSION: u16 = 1;

    fn id() -> &'static str {
        Self::ID
    }
}

pub trait HostEvent: BinEvent {}

pub trait UiEvent: BinEvent {}

pub trait AgentRequestContract: BinEvent {}

#[vmux_api::ui_event(Copy, Default)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::component::Component))]
pub struct PageReady;

#[cfg(test)]
mod tests {
    use super::*;

    #[vmux_api::ui_event]
    struct DerivedOpenRequest;

    #[vmux_api::ui_event(version = 2)]
    struct ExplicitOpenRequest;

    #[vmux_api::host_event(version = 3)]
    struct TestEvent;

    #[vmux_api::host_event]
    struct FirstEvent;

    #[vmux_api::host_event]
    struct SecondEvent;

    #[vmux_api::agent]
    struct OpenAgentRequest;

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
    fn derive_infers_event_contract_from_type_and_family() {
        assert_eq!(DerivedOpenRequest::id(), "derived_open@1");
        assert_eq!(DerivedOpenRequest::NAME, "derived_open");
        assert_eq!(DerivedOpenRequest::PERMISSION, "DerivedOpenRequest");
    }

    #[test]
    fn derive_accepts_explicit_event_contract() {
        assert_eq!(ExplicitOpenRequest::id(), "explicit_open@2");
        assert_eq!(ExplicitOpenRequest::NAME, "explicit_open");
    }

    #[test]
    fn agent_contract_uses_the_same_typed_id_contract() {
        assert_eq!(OpenAgentRequest::id(), "open_agent@1");
        assert_eq!(OpenAgentRequest::PERMISSION, "OpenAgentRequest");
    }
}
