//! Vendored [DioxusLabs/components `preview/src/components`](https://github.com/DioxusLabs/components/tree/ccdb07f69383de008a0afadda0e5ab7ec14c1a9c/preview/src/components) @ `ccdb07f` (`component.rs` + `style.css` per widget), plus vmux chrome (`UiRow`, `UiStack`, [`merge_class`], …) and [`icon`] (primitives re-export).
//!
//! Gallery-only folders live under `components/<name>/`; vmux helpers are flat modules (`row`, `stack`, …).

pub mod divider;
pub mod input_shell;
pub mod panel;
pub mod row;
pub mod stack;
pub mod text;
pub mod util;

pub mod accordion;
pub mod alert_dialog;
pub mod aspect_ratio;
pub mod avatar;
pub mod badge;
pub mod button;
pub mod calendar;
pub mod card;
pub mod checkbox;
pub mod collapsible;
pub mod context_menu;
pub mod date_picker;
pub mod dialog;
pub mod drag_and_drop_list;
pub mod dropdown_menu;
pub mod hover_card;
pub mod icon;
pub mod input;
pub mod label;
pub mod menubar;
pub mod navbar;
pub mod pagination;
pub mod popover;
pub mod progress;
pub mod radio_group;
pub mod scroll_area;
pub mod select;
pub mod separator;
pub mod sheet;
pub mod sidebar;
pub mod skeleton;
pub mod slider;
pub mod switch;
pub mod tabs;
pub mod textarea;
pub mod toast;
pub mod toggle;
pub mod toggle_group;
pub mod toolbar;
pub mod tooltip;
pub mod virtual_list;

pub use divider::{UiDivider, UiDividerVariant};
pub use input_shell::UiInputShell;
pub use panel::UiPanel;
pub use row::UiRow;
pub use stack::UiStack;
pub use text::{UiText, UiTextSize, UiTextTone};
pub use util::merge_class;
