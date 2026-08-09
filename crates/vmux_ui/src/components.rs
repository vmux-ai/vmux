pub mod accordion;
pub mod alert_dialog;
#[cfg(web)]
pub mod app;
pub mod aspect_ratio;
pub mod avatar;
pub mod badge;
pub mod button;
pub mod calendar;
pub mod card;
pub mod checkbox;
pub mod collapsible;
#[cfg(web)]
pub mod context_menu;
pub mod date_picker;
pub mod dialog;
pub mod drag_and_drop_list;
pub mod dropdown_menu;
#[cfg(web)]
pub mod gallery;
#[cfg(web)]
pub mod gallery_demos;
pub mod hover_card;
pub mod icon;
pub mod input;
pub mod label;
pub mod manager;
pub mod menubar;
#[cfg(web)]
pub mod navbar;
pub mod pagination;
pub mod popover;
pub mod progress;
pub mod prompt_box;
pub mod prompt_composer;
pub mod prompt_media_options;
pub mod radio_group;
pub mod scroll_area;
pub mod select;
pub mod separator;
pub mod sheet;
#[cfg(web)]
pub mod sidebar;
pub mod skeleton;
pub mod slider;
pub mod start_hero;
pub mod switch;
pub mod tabs;
pub mod text;
pub mod textarea;
pub mod toast;
pub mod toggle;
pub mod toggle_group;
pub mod toolbar;
pub mod tooltip;
pub mod virtual_list;

pub use crate::util::merge_class;
pub use text::{UiText, UiTextSize, UiTextTone};

#[cfg(test)]
mod naming_policy {
    use std::path::Path;

    /// A `#[component]` is written as an element — `Foo {}` — so its name has to read like one.
    /// Rust's own lints cannot say this: every page carries `#![allow(non_snake_case)]` to permit
    /// the convention in the first place, which switches off the only check in the area and makes
    /// a lower-case component compile silently. The convention is the oracle, so the scan is the
    /// only way to hold it.
    #[test]
    fn every_component_is_named_like_an_element() {
        let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates dir");
        let mut offenders = Vec::new();
        walk_rs_files(crates_dir, &mut |path, source| {
            for (index, name) in component_names(source) {
                if !name.starts_with(char::is_uppercase) {
                    offenders.push(format!("{}:{}: {name}", path.display(), index + 1));
                }
            }
        });
        assert!(
            offenders.is_empty(),
            "components must be PascalCase so they read as elements in rsx:\n{}",
            offenders.join("\n")
        );
    }

    /// A function returning `Element` renders UI, so it is a component and must say so.
    ///
    /// The naming check above cannot see these: a helper never claimed to be a component, so
    /// there is nothing to check the name of. That is exactly how 47 of them accumulated. The
    /// cost is not cosmetic — a helper is inlined into its caller's scope, so it re-runs whenever
    /// the caller does and can never skip on unchanged inputs.
    #[test]
    fn nothing_returns_an_element_without_being_a_component() {
        let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates dir");
        let mut offenders = Vec::new();
        walk_rs_files(crates_dir, &mut |path, source| {
            // This file quotes offending code in its own fixtures.
            if path.ends_with("components.rs") {
                return;
            }
            for (index, name) in element_fns_without_component(source) {
                offenders.push(format!("{}:{}: {name}", path.display(), index + 1));
            }
        });
        assert!(
            offenders.is_empty(),
            "these return Element, so they are components — add #[component], name them \
             PascalCase, take owned props, and render them as `Foo {{ .. }}`:\n{}",
            offenders.join("\n")
        );
    }

    /// Functions whose signature returns `Element` and that carry no `#[component]` above them.
    fn element_fns_without_component(source: &str) -> Vec<(usize, String)> {
        let lines: Vec<&str> = source.lines().collect();
        let mut found = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let Some(rest) = trimmed.strip_prefix("fn ").or_else(|| {
                trimmed
                    .strip_prefix("pub fn ")
                    .or_else(|| trimmed.strip_prefix("pub(crate) fn "))
            }) else {
                continue;
            };
            // The return type may sit on this line or after a wrapped parameter list.
            let signature: String = lines[index..lines.len().min(index + 14)].join("\n");
            let Some(head) = signature.split_once('{').map(|(head, _)| head) else {
                continue;
            };
            if !head.contains("-> Element") {
                continue;
            }
            let annotated = lines[..index].iter().rev().try_fold(false, |_, previous| {
                let previous = previous.trim();
                if previous == "#[component]" {
                    return Err(true);
                }
                if previous.starts_with('#') || previous.starts_with("//") {
                    return Ok(false);
                }
                Err(false)
            });
            if annotated == Err(true) {
                continue;
            }
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                found.push((index, name));
            }
        }
        found
    }

    /// Names of `#[component]`-annotated functions, with the line the name sits on.
    fn component_names(source: &str) -> Vec<(usize, String)> {
        let lines: Vec<&str> = source.lines().collect();
        let mut found = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            if line.trim() != "#[component]" {
                continue;
            }
            // Attributes and doc comments both sit between the marker and the signature, and
            // several components in this crate put the doc comment second.
            let Some((offset, signature)) =
                lines[index + 1..]
                    .iter()
                    .enumerate()
                    .find(|(_, candidate)| {
                        let candidate = candidate.trim_start();
                        !candidate.starts_with('#') && !candidate.starts_with("//")
                    })
            else {
                continue;
            };
            let Some(rest) = signature.split("fn ").nth(1) else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                found.push((index + 1 + offset, name));
            }
        }
        found
    }

    fn walk_rs_files(dir: &Path, visit: &mut dyn FnMut(&Path, &str)) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|name| name.to_str()) == Some("target") {
                    continue;
                }
                walk_rs_files(&path, visit);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs")
                && let Ok(source) = std::fs::read_to_string(&path)
            {
                visit(&path, &source);
            }
        }
    }

    /// The scan has to actually see a violation, or it passes for the wrong reason.
    #[test]
    fn the_scan_catches_a_lower_case_component() {
        let source = "#[component]\nfn my_widget() -> Element { rsx! {} }\n";
        let found = component_names(source);
        assert_eq!(found.len(), 1, "expected one component, got {found:?}");
        assert_eq!(found[0].1, "my_widget");
        assert!(!found[0].1.starts_with(char::is_uppercase));
    }

    /// Attributes and doc comments both sit between the marker and the signature. Missing either
    /// makes the scan skip the component entirely, so it would pass by seeing nothing.
    #[test]
    fn the_scan_looks_past_attributes_and_doc_comments() {
        for between in ["#[allow(non_snake_case)]", "/// Doc.", "// Note."] {
            let source =
                format!("#[component]\n{between}\npub fn Widget() -> Element {{ rsx! {{}} }}\n");
            let found = component_names(&source);
            assert_eq!(found.len(), 1, "missed the component after {between:?}");
            assert_eq!(found[0].1, "Widget");
        }
    }
}
