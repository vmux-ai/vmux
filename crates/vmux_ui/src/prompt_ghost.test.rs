use super::*;

#[test]
fn prompt_example_index_never_repeats_current() {
    for current in 0..4 {
        assert_ne!(
            distinct_prompt_example_index(4, Some(current), current),
            current
        );
    }
}

#[test]
fn prompt_typewriter_resets_after_pause() {
    let full = 12;
    assert_eq!(
        next_prompt_typed_count(full + PROMPT_PAUSE_TICKS - 1, full),
        Some(full + PROMPT_PAUSE_TICKS)
    );
    assert_eq!(
        next_prompt_typed_count(full + PROMPT_PAUSE_TICKS, full),
        None
    );
}
