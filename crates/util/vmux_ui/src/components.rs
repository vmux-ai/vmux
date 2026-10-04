pub mod agent_menu;
pub mod alert_dialog;
pub mod avatar;
pub mod badge;
pub mod button;
pub mod card;
pub mod checkbox;
pub mod composer;
pub mod composer_bar;
pub mod context_menu;
pub mod dialog;
pub mod effort_menu;
pub mod icon;
pub mod inline_edit;
pub mod input;
pub mod manager;
pub mod mcp_menu;
pub mod model_menu;
pub mod permission_menu;
pub mod progress;
pub mod project_picker;
pub mod prompt_box;
pub mod prompt_media_options;
pub mod select;
pub mod skeleton;
pub mod start_hero;
pub mod switch;
pub mod textarea;
pub mod tree_row;

#[cfg(test)]
mod naming_policy {
    use std::path::Path;

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

    #[test]
    fn nothing_returns_an_element_without_being_a_component() {
        let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates dir");
        let mut offenders = Vec::new();
        walk_rs_files(crates_dir, &mut |path, source| {
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

    #[test]
    fn static_component_styles_use_tailwind() {
        let components_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/components");
        let mut offenders = Vec::new();
        walk_rs_files(&components_dir, &mut |path, source| {
            for (index, line) in source.lines().enumerate() {
                let line = line.trim_start();
                if static_style_literal(line) || static_css_attribute(line) {
                    offenders.push(format!("{}:{}: {}", path.display(), index + 1, line.trim()));
                }
            }
        });
        assert!(
            offenders.is_empty(),
            "static component styles belong in Tailwind classes:\n{}",
            offenders.join("\n")
        );
    }

    fn static_style_literal(line: &str) -> bool {
        let Some(value) = line.strip_prefix("style: \"") else {
            return false;
        };
        !value.contains('{')
    }

    fn static_css_attribute(line: &str) -> bool {
        const PREFIXES: &[&str] = &[
            "background:",
            "background_color:",
            "border:",
            "border_color:",
            "border_radius:",
            "border_style:",
            "border_width:",
            "bottom:",
            "box_shadow:",
            "box_sizing:",
            "color:",
            "cursor:",
            "display:",
            "flex:",
            "flex_basis:",
            "flex_direction:",
            "flex_grow:",
            "flex_shrink:",
            "flex_wrap:",
            "font_family:",
            "font_size:",
            "font_style:",
            "font_weight:",
            "gap:",
            "grid:",
            "inset:",
            "justify_content:",
            "left:",
            "letter_spacing:",
            "line_height:",
            "margin:",
            "margin_bottom:",
            "margin_left:",
            "margin_right:",
            "margin_top:",
            "max_height:",
            "max_width:",
            "min_height:",
            "min_width:",
            "opacity:",
            "overflow:",
            "overflow_x:",
            "overflow_y:",
            "padding:",
            "padding_bottom:",
            "padding_left:",
            "padding_right:",
            "padding_top:",
            "position:",
            "right:",
            "text_align:",
            "text_decoration:",
            "top:",
            "transform:",
            "transition:",
            "visibility:",
            "white_space:",
            "z_index:",
        ];
        PREFIXES.iter().any(|prefix| {
            line.strip_prefix(prefix)
                .is_some_and(|value| value.trim_start().starts_with('"') && !value.contains('{'))
        })
    }

    fn element_fns_without_component(source: &str) -> Vec<(usize, String)> {
        let lines: Vec<&str> = source.lines().collect();
        let mut found = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let rest = ["fn ", "pub fn ", "pub(crate) fn ", "pub(super) fn "]
                .iter()
                .find_map(|prefix| trimmed.strip_prefix(prefix));
            let Some(rest) = rest else {
                continue;
            };
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

    fn component_names(source: &str) -> Vec<(usize, String)> {
        let lines: Vec<&str> = source.lines().collect();
        let mut found = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            if line.trim() != "#[component]" {
                continue;
            }
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

    #[test]
    fn the_scan_catches_a_lower_case_component() {
        let source = "#[component]\nfn my_widget() -> Element { rsx! {} }\n";
        let found = component_names(source);
        assert_eq!(found.len(), 1, "expected one component, got {found:?}");
        assert_eq!(found[0].1, "my_widget");
        assert!(!found[0].1.starts_with(char::is_uppercase));
    }

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
