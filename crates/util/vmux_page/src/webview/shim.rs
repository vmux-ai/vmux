use crate::HistorySwipe;
use vmux_ui::components::popup::Surface;
use vmux_ui::motion::Motion;

pub(super) struct Shim;

impl Shim {
    pub(super) fn source() -> String {
        WRY_HOST_SHIM
            .replace(
                "__VMUX_HISTORY_SWIPE__",
                &HistorySwipe::script("postMessage('history:' + direction);"),
            )
            .replace("__VMUX_POPUP_RUNTIME__", &Surface::runtime())
            .replace(
                "__VMUX_MOTION_FEEDBACK_MS__",
                &Motion::FEEDBACK_MS.to_string(),
            )
            .replace(
                "__VMUX_MOTION_CONTENT_MS__",
                &Motion::CONTENT_MS.to_string(),
            )
            .replace("__VMUX_MOTION_LAYOUT_MS__", &Motion::LAYOUT_MS.to_string())
            .replace("__VMUX_MOTION_MAX_MS__", &Motion::MAX_MS.to_string())
    }
}

pub(crate) const WRY_HOST_SHIM: &str = include_str!("wry_host_shim.js");

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    struct RuntimeProbe;

    impl RuntimeProbe {
        fn verify(script: &str) {
            let mut child = Command::new("node")
                .args(["-e", script])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Node is required for the page runtime test");
            child
                .stdin
                .take()
                .unwrap()
                .write_all(Shim::source().as_bytes())
                .unwrap();
            let result = child.wait_with_output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }

    #[test]
    fn closing_a_middle_tab_fades_that_tab_and_moves_its_successor_without_delaying_layout() {
        RuntimeProbe::verify(include_str!("layout_movement_test.js"));
    }

    #[test]
    fn scroll_notifications_do_not_wait_for_native_replies_and_coalesce_per_frame() {
        RuntimeProbe::verify(include_str!("scroll_events_test.js"));
    }

    #[test]
    fn sticky_reference_navigation_respects_the_containers_scroll_padding() {
        RuntimeProbe::verify(include_str!("scroll_section_test.js"));
    }

    #[test]
    fn content_growth_and_scroll_clamping_preserve_position_before_the_next_input() {
        RuntimeProbe::verify(include_str!("scroll_retention_test.js"));
    }

    #[test]
    fn first_layout_presents_without_waiting_for_background_data_or_fonts() {
        RuntimeProbe::verify(include_str!("presentation_test.js"));
    }

    #[test]
    fn composer_handoff_preserves_final_layout_and_does_not_delay_navigation() {
        RuntimeProbe::verify(include_str!("composer_handoff_test.js"));
    }

    #[test]
    fn composer_handoff_uses_native_browser_layout_and_animation() {
        RuntimeProbe::verify(include_str!("composer_handoff_browser_test.js"));
    }
}
