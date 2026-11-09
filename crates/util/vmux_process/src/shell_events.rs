use vte::{Parser, Perform};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    CommandStart,
    Directory(String),
    CommandEnd(Option<i32>),
}

pub struct ShellEvents {
    parser: Parser,
}

impl ShellEvents {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<ShellEvent> {
        let mut collector = Collector::default();
        self.parser.advance(&mut collector, bytes);
        collector.events
    }
}

#[derive(Default)]
struct Collector {
    events: Vec<ShellEvent>,
}

impl Perform for Collector {
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.first().copied() == Some(b"7".as_slice()) {
            let mut bytes = Vec::new();
            for (index, parameter) in params[1..].iter().enumerate() {
                if index > 0 {
                    bytes.push(b';');
                }
                bytes.extend_from_slice(parameter);
            }
            if bytes.len() > 4096 {
                return;
            }
            let Ok(value) = std::str::from_utf8(&bytes) else {
                return;
            };
            let Ok(url) = url::Url::parse(value) else {
                return;
            };
            let Ok(path) = url.to_file_path() else {
                return;
            };
            if path.is_absolute() && !value.chars().any(char::is_control) {
                self.events
                    .push(ShellEvent::Directory(path.to_string_lossy().into_owned()));
            }
            return;
        }
        if params.first().copied() != Some(b"133".as_slice()) {
            return;
        }
        let kind = params.get(1).copied();
        if kind == Some(b"C".as_slice()) {
            self.events.push(ShellEvent::CommandStart);
        } else if kind == Some(b"D".as_slice()) {
            let exit = params
                .get(2)
                .and_then(|p| std::str::from_utf8(p).ok())
                .and_then(|s| s.trim().parse::<i32>().ok());
            self.events.push(ShellEvent::CommandEnd(exit));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_directory_decodes_escaped_paths_and_split_frames() {
        let mut scanner = ShellEvents::new();
        assert!(
            scanner
                .feed(b"\x1b]7;file://localhost/tmp/a%20b%23%3F")
                .is_empty()
        );
        assert_eq!(
            scanner.feed(b"%25;name\x07"),
            vec![ShellEvent::Directory("/tmp/a b#?%;name".into())]
        );
        assert!(
            scanner
                .feed(b"\x1b]7;https://example.com/path\x07")
                .is_empty()
        );
    }

    fn esc(seq: &str) -> Vec<u8> {
        seq.replace("\\e", "\u{1b}")
            .replace("\\a", "\u{07}")
            .into_bytes()
    }

    #[test]
    fn detects_command_start() {
        let mut s = ShellEvents::new();
        assert_eq!(s.feed(&esc("\\e]133;C\\a")), vec![ShellEvent::CommandStart]);
    }

    #[test]
    fn detects_command_end_with_exit_code() {
        let mut s = ShellEvents::new();
        assert_eq!(
            s.feed(&esc("\\e]133;D;0\\a")),
            vec![ShellEvent::CommandEnd(Some(0))]
        );
        assert_eq!(
            s.feed(&esc("\\e]133;D;130\\a")),
            vec![ShellEvent::CommandEnd(Some(130))]
        );
    }

    #[test]
    fn command_end_without_code_is_none() {
        let mut s = ShellEvents::new();
        assert_eq!(
            s.feed(&esc("\\e]133;D\\a")),
            vec![ShellEvent::CommandEnd(None)]
        );
    }

    #[test]
    fn accepts_st_terminator() {
        let mut s = ShellEvents::new();
        assert_eq!(
            s.feed(&esc("\\e]133;D;0\\e\\")),
            vec![ShellEvent::CommandEnd(Some(0))]
        );
    }

    #[test]
    fn reassembles_sequence_split_across_feeds() {
        let mut s = ShellEvents::new();
        assert_eq!(s.feed(&esc("\\e]133;D")), vec![]);
        assert_eq!(s.feed(&esc(";0\\a")), vec![ShellEvent::CommandEnd(Some(0))]);
    }

    #[test]
    fn ignores_other_osc_and_plain_text() {
        let mut s = ShellEvents::new();
        assert_eq!(s.feed(&esc("\\e]0;my title\\ahello world\n")), vec![]);
    }

    #[test]
    fn emits_start_then_end_in_order() {
        let mut s = ShellEvents::new();
        assert_eq!(
            s.feed(&esc("\\e]133;C\\als -la\n\\e]133;D;0\\a")),
            vec![ShellEvent::CommandStart, ShellEvent::CommandEnd(Some(0))]
        );
    }
}
