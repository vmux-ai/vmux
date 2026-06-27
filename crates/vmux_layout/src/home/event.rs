/// Page→host request: the `vmux://home/` page emits this on mount to ask the host
/// for its launcher entries. The host answers with a `CommandBarOpenEvent`.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct HomeDataRequest;
