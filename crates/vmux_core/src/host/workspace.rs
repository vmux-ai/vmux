//! When the workspace settles, for everything that has to run around it.
//!
//! The sets are declared here and populated by whichever crate owns the workspace, the same way
//! [`PageOpenSet`](crate::page_open::PageOpenSet) is declared here and populated by the pages. Four
//! crates already order against them, and a crate that only needs to run *after* tabs are resolved
//! should not have to depend on the one that resolves them.

use bevy::prelude::SystemSet;

/// Where tab commands are answered. Anything reading the set of tabs runs after this.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TabCommandSet;

/// Where stack commands are answered. Anything reading the set of stacks runs after this.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StackCommandSet;

/// Where the focused stack is worked out.
///
/// Anything reading focus runs `.after` this; anything that wants its own change taken into account
/// runs `.before` it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComputeFocusSet;
