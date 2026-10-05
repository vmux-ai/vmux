pub trait UiState: crate::HostEvent + Clone + Send + Sync + 'static {
    type Update: Clone + Send + Sync + 'static;

    fn from_updates(previous: Option<Self>, updates: Vec<Self::Update>) -> Self;
    fn retained(&self) -> Option<Self>;
}

pub trait UiStateProjection<P>: Default {
    fn apply(&mut self, patch: P);
}

pub trait UiStatePatch<T>: 'static {
    fn payload(&self) -> Option<&T>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[vmux_api::ui_state_patch(Default)]
    struct TestPatch {
        number: Option<u32>,
        text: Option<String>,
    }

    #[test]
    fn derives_typed_patch_access() {
        let number = TestPatch::from(5u32);
        let text = TestPatch::from(String::from("ready"));
        assert_eq!(<TestPatch as UiStatePatch<u32>>::payload(&number), Some(&5),);
        let text = <TestPatch as UiStatePatch<String>>::payload(&text);
        assert_eq!(text.map(String::as_str), Some("ready"));
    }

    #[vmux_api::ui_state(Default)]
    struct TestSnapshot {
        value: u32,
    }

    #[vmux_api::ui_state_patch(Default)]
    struct TestProjectedPatch {
        value: Option<u32>,
        text: Option<String>,
    }

    #[vmux_api::ui_state(Default, Eq, patch = TestProjectedPatch)]
    struct TestProjectedState {
        value: u32,
        text: String,
    }

    impl UiStateProjection<TestProjectedPatch> for TestProjectedState {
        fn apply(&mut self, patch: TestProjectedPatch) {
            if let Some(value) = patch.value {
                self.value = value;
            }
            if let Some(text) = patch.text {
                self.text = text;
            }
        }
    }

    #[test]
    fn derives_snapshot_state_without_patch_fields() {
        fn assert_state<T: UiState>() {}
        assert_state::<TestSnapshot>();
        assert_eq!(TestSnapshot::default().value, 0);
        let state = TestSnapshot::from_updates(None, vec![TestSnapshot { value: 9 }]);
        assert_eq!(state.value, 9);
        assert_eq!(state.retained().map(|state| state.value), Some(9));
    }

    #[test]
    fn projected_state_retains_fields_between_update_batches() {
        let state = TestProjectedState::from_updates(None, vec![7u32.into()]);
        let state =
            TestProjectedState::from_updates(Some(state), vec![String::from("ready").into()]);

        assert_eq!(state.value, 7);
        assert_eq!(state.text, "ready");
    }
}
