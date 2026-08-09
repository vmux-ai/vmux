use super::*;

#[test]
fn session_components_default_constructible() {
    let _ = AgentMessages::default();
    let _ = AgentApprovalPolicy::default();
    let _ = PromptQueue::default();
}

#[test]
fn approval_policy_normalizes_agent_tool_identifiers() {
    let mut policy = AgentApprovalPolicy::default();
    policy.allow("mcp__vmux__run");

    assert!(policy.allows("mcp.vmux.run"));
    assert!(!policy.allows("mcp.other.run"));
}

#[test]
fn prompt_queue_ready_gate() {
    let mut q = PromptQueue::default();
    assert!(!q.ready(true));
    q.enqueue("a".into());
    assert!(q.ready(true));
    assert!(!q.ready(false));
    q.paused = true;
    assert!(!q.ready(true));
}

#[test]
fn take_next_preserves_fifo_without_flush() {
    let mut q = PromptQueue::default();
    q.enqueue("first".into());
    q.enqueue("second".into());
    assert_eq!(
        q.take_next().map(|prompt| prompt.text),
        Some("first".to_string())
    );
    assert_eq!(
        q.items.front().map(|item| item.text.as_str()),
        Some("second")
    );
}

#[test]
fn take_next_merges_all_items_for_flush() {
    let mut q = PromptQueue::default();
    q.enqueue("first".into());
    q.enqueue("second".into());

    assert!(q.request_flush());
    assert_eq!(
        q.take_next().map(|prompt| prompt.text),
        Some("first\n\nsecond".to_string())
    );
    assert!(q.items.is_empty());
    assert!(!q.flush_pending);
}

#[test]
fn take_next_merges_attachments_for_flush() {
    let mut q = PromptQueue::default();
    q.enqueue_with_attachments(
        String::new(),
        vec![AgentAttachment {
            path: "/tmp/a.png".into(),
            name: "a.png".into(),
            mime_type: "image/png".into(),
            size: 3,
        }],
    );
    q.enqueue_with_attachments(
        "describe both".into(),
        vec![AgentAttachment {
            path: "/tmp/b.png".into(),
            name: "b.png".into(),
            mime_type: "image/png".into(),
            size: 4,
        }],
    );

    assert!(q.request_flush());
    let prompt = q.take_next().unwrap();
    assert_eq!(prompt.text, "describe both");
    assert_eq!(prompt.attachments.len(), 2);
}

#[test]
fn enqueue_preserves_pending_flush() {
    let mut q = PromptQueue::default();
    q.enqueue("first".into());
    assert!(q.request_flush());

    q.enqueue("second".into());

    assert!(q.flush_pending);
    assert_eq!(
        q.take_next().map(|prompt| prompt.text),
        Some("first\n\nsecond".to_string())
    );
}

#[test]
fn cancel_flush_clears_pending_flush() {
    let mut q = PromptQueue::default();
    q.enqueue("first".into());
    assert!(q.request_flush());

    q.cancel_flush();

    assert!(!q.flush_pending);
}

#[test]
fn clear_resets_queue_state() {
    let mut q = PromptQueue::default();
    q.enqueue("first".into());
    assert!(q.request_flush());
    q.paused = true;

    q.clear();

    assert!(q.items.is_empty());
    assert!(!q.paused);
    assert!(!q.flush_pending);
}

#[test]
fn resume_resets_pause_and_flush() {
    let mut q = PromptQueue::default();
    q.enqueue("first".into());
    assert!(q.request_flush());
    q.paused = true;

    q.resume();

    assert!(!q.paused);
    assert!(!q.flush_pending);
}

#[test]
fn remove_targets_stable_id() {
    let mut q = PromptQueue::default();
    q.enqueue("first".into());
    q.enqueue("second".into());
    let second_id = q.items[1].id;

    assert!(q.remove(second_id));
    assert_eq!(q.items.len(), 1);
    assert_eq!(q.items[0].text, "first");
    assert!(!q.remove(second_id));
}

#[test]
fn removing_last_item_resets_queue_state() {
    let mut q = PromptQueue::default();
    q.enqueue("first".into());
    let id = q.items[0].id;
    assert!(q.request_flush());
    q.paused = true;

    assert!(q.remove(id));
    assert!(q.items.is_empty());
    assert!(!q.paused);
    assert!(!q.flush_pending);
}
