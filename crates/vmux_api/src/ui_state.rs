pub trait UiState: Clone + 'static {
    type Patch: 'static;

    fn sequence(&self) -> u64;
    fn patches(&self) -> &[Self::Patch];
}

pub trait UiStatePatch<T>: 'static {
    fn payload(&self) -> Option<&T>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, vmux_api::UiState)]
    struct TestState {
        sequence: u64,
        patches: Vec<TestPatch>,
    }

    #[derive(Clone, vmux_api::UiStatePatch)]
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
        assert_eq!(
            <TestPatch as UiStatePatch<u32>>::payload(&state.patches()[0]),
            Some(&5),
        );
        let text = <TestPatch as UiStatePatch<String>>::payload(&state.patches()[1]);
        assert_eq!(text.map(String::as_str), Some("ready"));
    }
}
