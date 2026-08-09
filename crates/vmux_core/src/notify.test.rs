use super::*;

#[test]
fn agent_attention_carries_optional_text() {
    let a = AgentAttention {
        entity: Entity::PLACEHOLDER,
        title: Some("done".into()),
        body: None,
    };
    assert_eq!(a.title.as_deref(), Some("done"));
    assert!(a.body.is_none());
}
