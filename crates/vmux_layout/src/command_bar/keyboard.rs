#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtrlEditAction {
    Home,
    End,
    Forward,
    Back,
    Delete,
    Backspace,
    DeleteWord,
    DeleteToBeginning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtrlKeyCapture {
    Ignore,
    Edit(CtrlEditAction),
    PassToDioxus,
    RerouteToDioxus,
}

pub fn ctrl_key_capture_for_code(code: &str) -> CtrlKeyCapture {
    match code {
        "KeyA" => CtrlKeyCapture::Edit(CtrlEditAction::Home),
        "KeyE" => CtrlKeyCapture::Edit(CtrlEditAction::End),
        "KeyF" => CtrlKeyCapture::Edit(CtrlEditAction::Forward),
        "KeyB" => CtrlKeyCapture::Edit(CtrlEditAction::Back),
        "KeyD" => CtrlKeyCapture::Edit(CtrlEditAction::Delete),
        "KeyH" => CtrlKeyCapture::Edit(CtrlEditAction::Backspace),
        "KeyW" => CtrlKeyCapture::Edit(CtrlEditAction::DeleteWord),
        "KeyU" => CtrlKeyCapture::Edit(CtrlEditAction::DeleteToBeginning),
        "KeyC" | "KeyJ" | "KeyK" | "KeyN" | "KeyP" => CtrlKeyCapture::PassToDioxus,
        _ => CtrlKeyCapture::Ignore,
    }
}

pub fn ignore_physical_rerouted_ctrl_keydown(code: &str, is_synthetic: bool) -> bool {
    !is_synthetic
        && matches!(
            ctrl_key_capture_for_code(code),
            CtrlKeyCapture::RerouteToDioxus
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_j_and_k_pass_to_dioxus_without_synthetic_reroute() {
        assert_eq!(
            ctrl_key_capture_for_code("KeyJ"),
            CtrlKeyCapture::PassToDioxus
        );
        assert_eq!(
            ctrl_key_capture_for_code("KeyK"),
            CtrlKeyCapture::PassToDioxus
        );
    }

    #[test]
    fn ctrl_n_p_c_pass_to_dioxus() {
        assert_eq!(
            ctrl_key_capture_for_code("KeyN"),
            CtrlKeyCapture::PassToDioxus
        );
        assert_eq!(
            ctrl_key_capture_for_code("KeyP"),
            CtrlKeyCapture::PassToDioxus
        );
        assert_eq!(
            ctrl_key_capture_for_code("KeyC"),
            CtrlKeyCapture::PassToDioxus
        );
    }

    #[test]
    fn ctrl_text_edit_keys_are_handled_by_capture_listener() {
        assert_eq!(
            ctrl_key_capture_for_code("KeyA"),
            CtrlKeyCapture::Edit(CtrlEditAction::Home)
        );
        assert_eq!(
            ctrl_key_capture_for_code("KeyU"),
            CtrlKeyCapture::Edit(CtrlEditAction::DeleteToBeginning)
        );
    }

    #[test]
    fn physical_ctrl_j_k_are_not_suppressed_for_synthetic_reroute() {
        assert!(!ignore_physical_rerouted_ctrl_keydown("KeyJ", false));
        assert!(!ignore_physical_rerouted_ctrl_keydown("KeyK", false));
        assert!(!ignore_physical_rerouted_ctrl_keydown("KeyJ", true));
        assert!(!ignore_physical_rerouted_ctrl_keydown("KeyK", true));
        assert!(!ignore_physical_rerouted_ctrl_keydown("KeyN", false));
        assert!(!ignore_physical_rerouted_ctrl_keydown("KeyP", false));
    }
}
