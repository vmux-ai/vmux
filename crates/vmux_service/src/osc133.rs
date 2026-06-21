use vte::{Parser, Perform};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Osc133Event {
    CommandStart,
    CommandEnd(Option<i32>),
}

pub struct Osc133Scanner {
    parser: Parser,
}

impl Osc133Scanner {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Osc133Event> {
        let mut collector = Collector::default();
        self.parser.advance(&mut collector, bytes);
        collector.events
    }
}

#[derive(Default)]
struct Collector {
    events: Vec<Osc133Event>,
}

impl Perform for Collector {
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.first().copied() != Some(b"133".as_slice()) {
            return;
        }
        let kind = params.get(1).copied();
        if kind == Some(b"C".as_slice()) {
            self.events.push(Osc133Event::CommandStart);
        } else if kind == Some(b"D".as_slice()) {
            let exit = params
                .get(2)
                .and_then(|p| std::str::from_utf8(p).ok())
                .and_then(|s| s.trim().parse::<i32>().ok());
            self.events.push(Osc133Event::CommandEnd(exit));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn esc(seq: &str) -> Vec<u8> {
        seq.replace("\\e", "\u{1b}")
            .replace("\\a", "\u{07}")
            .into_bytes()
    }

    #[test]
    fn detects_command_start() {
        let mut s = Osc133Scanner::new();
        assert_eq!(s.feed(&esc("\\e]133;C\\a")), vec![Osc133Event::CommandStart]);
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
        assert_eq!(s.feed(&esc(";0\\a")), vec![Osc133Event::CommandEnd(Some(0))]);
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
}
