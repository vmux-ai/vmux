pub struct HistorySwipe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryDirection {
    Back,
    Forward,
}

impl HistoryDirection {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "back" => Some(Self::Back),
            "forward" => Some(Self::Forward),
            _ => None,
        }
    }
}

impl HistorySwipe {
    pub fn script(callback: &str) -> String {
        SCRIPT
            .replace("__VMUX_HISTORY_CALLBACK__", callback)
            .replace(
                "__VMUX_HISTORY_FEEDBACK_MS__",
                &vmux_ui::motion::Motion::FEEDBACK_MS.to_string(),
            )
    }
}

const SCRIPT: &str = include_str!("history_swipe.js");

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    struct GestureProbe;

    impl GestureProbe {
        fn verify() {
            let mut child = Command::new("node")
                .args(["-e", include_str!("history_swipe_test.js")])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Node is required for the gesture runtime test");
            child
                .stdin
                .take()
                .unwrap()
                .write_all(
                    HistorySwipe::script("globalThis.navigations.push(direction);").as_bytes(),
                )
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[test]
    fn trackpad_gestures_ignore_startup_noise_and_respect_horizontal_scroll_boundaries() {
        GestureProbe::verify();
    }
}
