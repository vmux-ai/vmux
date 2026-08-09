use super::*;

fn esc(seq: &str) -> Vec<u8> {
    seq.replace("\\e", "\u{1b}")
        .replace("\\a", "\u{07}")
        .into_bytes()
}

#[test]
fn extracts_token_and_exit() {
    let mut s = RunMarkerScanner::new();
    assert_eq!(
        s.feed(&esc("\\e]6973;abc123;0\\a")),
        vec![RunMarker {
            token: "abc123".to_string(),
            exit: 0
        }]
    );
}

#[test]
fn extracts_nonzero_exit() {
    let mut s = RunMarkerScanner::new();
    assert_eq!(
        s.feed(&esc("\\e]6973;tok;130\\a")),
        vec![RunMarker {
            token: "tok".to_string(),
            exit: 130
        }]
    );
}

#[test]
fn accepts_st_terminator() {
    let mut s = RunMarkerScanner::new();
    assert_eq!(
        s.feed(&esc("\\e]6973;tok;1\\e\\")),
        vec![RunMarker {
            token: "tok".to_string(),
            exit: 1
        }]
    );
}

#[test]
fn reassembles_sequence_split_across_feeds() {
    let mut s = RunMarkerScanner::new();
    assert_eq!(s.feed(&esc("\\e]6973;tok")), vec![]);
    assert_eq!(
        s.feed(&esc(";7\\a")),
        vec![RunMarker {
            token: "tok".to_string(),
            exit: 7
        }]
    );
}

#[test]
fn ignores_osc133_and_other_osc() {
    let mut s = RunMarkerScanner::new();
    assert_eq!(s.feed(&esc("\\e]133;D;0\\a")), vec![]);
    assert_eq!(s.feed(&esc("\\e]0;window title\\a")), vec![]);
}

#[test]
fn ignores_plain_text() {
    let mut s = RunMarkerScanner::new();
    assert_eq!(s.feed(b"__VMUX_DONE_tok_0__\n"), vec![]);
}

#[test]
fn drops_marker_with_missing_or_bad_exit() {
    let mut s = RunMarkerScanner::new();
    assert_eq!(s.feed(&esc("\\e]6973;tok\\a")), vec![]);
    assert_eq!(s.feed(&esc("\\e]6973;tok;notanumber\\a")), vec![]);
}

#[test]
fn drops_marker_with_empty_token() {
    let mut s = RunMarkerScanner::new();
    assert_eq!(s.feed(&esc("\\e]6973;;0\\a")), vec![]);
}
