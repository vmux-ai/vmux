pub trait UiState: crate::HostEvent + Clone + Send + Sync + 'static {}

pub trait BatchedUiState: UiState {
    type Patch: Clone + Send + Sync + 'static;

    fn sequence(&self) -> u64;
    fn patches(&self) -> &[Self::Patch];
    fn from_parts(sequence: u64, patches: Vec<Self::Patch>) -> Self;
}

pub trait UiStatePatch<T>: 'static {
    fn payload(&self) -> Option<&T>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[vmux_api::ui_state(target = any)]
    struct TestState {
        sequence: u64,
        patches: Vec<TestPatch>,
    }

    #[vmux_api::ui_state_patch]
    enum TestPatch {
        Number(u32),
        Text(String),
    }

    #[test]
    fn derives_state_and_typed_patch_access() {
        let state = TestState {
            sequence: 7,
            patches: vec![5u32.into(), String::from("ready").into()],
        };
        assert_eq!(state.sequence(), 7);
        assert_eq!(state.patches().len(), 2);
        assert_eq!(TestState::from_parts(8, vec![9u32.into()]).sequence, 8);
        assert_eq!(
            <TestPatch as UiStatePatch<u32>>::payload(&state.patches()[0]),
            Some(&5),
        );
        let text = <TestPatch as UiStatePatch<String>>::payload(&state.patches()[1]);
        assert_eq!(text.map(String::as_str), Some("ready"));
    }

    #[vmux_api::ui_state(Default, target = any)]
    struct TestSnapshot {
        value: u32,
    }

    #[test]
    fn derives_snapshot_state_without_patch_fields() {
        fn assert_state<T: UiState>() {}
        assert_state::<TestSnapshot>();
        assert_eq!(TestSnapshot::default().value, 0);
    }
}
