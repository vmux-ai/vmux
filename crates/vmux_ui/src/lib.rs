//! UI design tokens for vmux: reusable colors, spacing, and related values for engine-rendered
//! chrome (loading bar, borders, …) and future UI surfaces.

pub mod design;

/// Common imports for design tokens.
pub mod prelude {
    pub use crate::design::color;
}
