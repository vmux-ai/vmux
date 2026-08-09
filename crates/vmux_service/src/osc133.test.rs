use super::*;

fn esc(seq: &str) -> Vec<u8> {
    seq.replace("\\e", "\u{1b}")
        .replace("\\a", "\u{07}")
        .into_bytes()
}

#[test]
fn detects_command_start() {
    let mut s = Osc133Scanner::new();
    assert_eq!(
        s.feed(&esc("\\e]133;C\\a")),
        vec![Osc133Event::CommandStart]
    );
}

#[test]
fn detects_command_end_with_exit_code() {
    let mut s = Osc133Scanner::new();
    assert_eq!(
        s.feed(&esc("\\e]133;D;0\\a")),
        vec![Osc133Event::CommandEnd(Some(0))]
    );
    assert_eq!(
        s.feed(&esc("\\e]133;D;130\\a")),
        vec![Osc133Event::CommandEnd(Some(130))]
    );
}

#[test]
fn command_end_without_code_is_none() {
    let mut s = Osc133Scanner::new();
    assert_eq!(
        s.feed(&esc("\\e]133;D\\a")),
        vec![Osc133Event::CommandEnd(None)]
    );
}

#[test]
fn accepts_st_terminator() {
    let mut s = Osc133Scanner::new();
    assert_eq!(
        s.feed(&esc("\\e]133;D;0\\e\\")),
        vec![Osc133Event::CommandEnd(Some(0))]
    );
}

#[test]
fn reassembles_sequence_split_across_feeds() {
    let mut s = Osc133Scanner::new();
    assert_eq!(s.feed(&esc("\\e]133;D")), vec![]);
    assert_eq!(
        s.feed(&esc(";0\\a")),
        vec![Osc133Event::CommandEnd(Some(0))]
    );
}

#[test]
fn ignores_other_osc_and_plain_text() {
    let mut s = Osc133Scanner::new();
    assert_eq!(s.feed(&esc("\\e]0;my title\\ahello world\n")), vec![]);
}

#[test]
fn emits_start_then_end_in_order() {
    let mut s = Osc133Scanner::new();
    assert_eq!(
        s.feed(&esc("\\e]133;C\\als -la\n\\e]133;D;0\\a")),
        vec![Osc133Event::CommandStart, Osc133Event::CommandEnd(Some(0))]
    );
}
