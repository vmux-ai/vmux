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
#[path = "osc133.test.rs"]
mod tests;
