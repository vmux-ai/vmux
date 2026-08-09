use super::*;

#[test]
fn shell_command_input_appends_carriage_return() {
    assert_eq!(shell_command_input("echo hi"), b"echo hi\r".to_vec());
}

#[test]
fn bracketed_paste_wraps_and_submits() {
    assert_eq!(
        bracketed_paste_input("hi", true),
        b"\x1b[200~hi\x1b[201~\r".to_vec()
    );
    assert_eq!(
        bracketed_paste_input("hi", false),
        b"\x1b[200~hi\x1b[201~".to_vec()
    );
}

#[test]
fn bracketed_paste_strips_terminator_and_handles_empty() {
    assert_eq!(
        bracketed_paste_input("a\x1b[201~b", false),
        b"\x1b[200~ab\x1b[201~".to_vec()
    );
    assert!(bracketed_paste_input("", true).is_empty());
    assert!(bracketed_paste_input("\x1b[201~", true).is_empty());
}
