//! Fixture crate root docs.

/// A documented struct.
pub struct Widget {
    /// The width field.
    pub width: u32,
}

/// A documented function.
pub fn make(width: u32) -> Widget {
    Widget { width }
}

/// A documented enum.
pub enum Mode {
    /// Fast mode.
    Fast,
    /// Slow mode.
    Slow,
}

struct Hidden;

#[doc(hidden)]
pub struct AlsoHidden;

/// A documented submodule.
pub mod inner {
    //! Inner module docs.

    /// Inner constant.
    pub const ANSWER: u32 = 42;
}
