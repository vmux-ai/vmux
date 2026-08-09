use super::format_crash_record;

#[test]
fn format_crash_record_contains_message_and_location() {
    let out = format_crash_record(
        "2026-06-16T12:00:00Z",
        "main",
        "crates/vmux_desktop/src/main.rs:42:9",
        "boom",
        "<backtrace>",
    );
    assert!(out.contains("boom"), "got {out}");
    assert!(
        out.contains("crates/vmux_desktop/src/main.rs:42:9"),
        "got {out}"
    );
    assert!(out.contains("thread=main"), "got {out}");
    assert!(out.contains("PANIC"), "got {out}");
}
