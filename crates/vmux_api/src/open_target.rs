#[vmux_api::contract(Copy, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaneDirection {
    #[default]
    Top,
    Right,
    Bottom,
    Left,
}

#[vmux_api::contract(Copy, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaneTarget {
    Existing,
    #[default]
    NewSplit,
}

#[vmux_api::contract(Copy, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaneOpenMode {
    InPlace,
    #[default]
    NewStack,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub enum OpenTarget {
    #[default]
    InPlace,
    InNewStack,
    InPane {
        direction: PaneDirection,
        target: PaneTarget,
        mode: PaneOpenMode,
    },
    InNewTab,
    InNewSpace,
}
