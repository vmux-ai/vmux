use vte::{Parser, Perform};

/// vmux-private OSC code emitted by the agent `run` wrapper to signal command
/// completion invisibly: `ESC ] 6973 ; <token> ; <exit> BEL`.
///
/// Distinct from OSC `133` so it never disturbs the OSC 133 command lifecycle
/// (which drives the vibe "armed" pane). The token travels inline with the exact
/// command, so completion is correlated per-run without a seq baseline.
pub const VMUX_RUN_OSC: &str = "6973";

/// A completed `run`, parsed from a [`VMUX_RUN_OSC`] escape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunMarker {
    pub token: String,
    pub exit: i32,
}

/// Scans a PTY byte stream for [`VMUX_RUN_OSC`] completion escapes, reassembling
/// sequences split across feeds.
pub struct RunMarkerScanner {
    parser: Parser,
}

impl RunMarkerScanner {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<RunMarker> {
        let mut collector = Collector::default();
        self.parser.advance(&mut collector, bytes);
        collector.markers
    }
}

impl Default for RunMarkerScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
struct Collector {
    markers: Vec<RunMarker>,
}

impl Perform for Collector {
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.first().copied() != Some(VMUX_RUN_OSC.as_bytes()) {
            return;
        }
        let token = params
            .get(1)
            .and_then(|p| std::str::from_utf8(p).ok())
            .filter(|t| !t.is_empty());
        let exit = params
            .get(2)
            .and_then(|p| std::str::from_utf8(p).ok())
            .and_then(|s| s.trim().parse::<i32>().ok());
        if let (Some(token), Some(exit)) = (token, exit) {
            self.markers.push(RunMarker {
                token: token.to_string(),
                exit,
            });
        }
    }
}

#[cfg(test)]
#[path = "run_marker.test.rs"]
mod tests;
