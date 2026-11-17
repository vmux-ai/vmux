use crate::Result;
use std::collections::HashSet;

pub(crate) struct Graph;

impl Graph {
    pub(crate) fn check(tree: &str) -> Result<()> {
        Self::packages(
            tree,
            &[
                "vmux_app",
                "vmux_ecs",
                "vmux_project",
                "vmux_extension",
                "vmux_flex",
                "vmux_layout",
                "vmux_lsp",
            ],
            &[
                "bevy",
                "bevy_cef",
                "bevy_cef_core",
                "bevy_winit",
                "vmux_profile",
                "vmux_editor",
                "vmux_ui",
                "vmux_pane",
                "wry",
            ],
        )
    }

    pub(crate) fn host(tree: &str) -> Result<()> {
        Self::packages(
            tree,
            &["vmux_app", "vmux_profile"],
            &[
                "bevy",
                "bevy_cef",
                "bevy_cef_core",
                "bevy_winit",
                "vmux_ecs",
                "vmux_editor",
                "vmux_ui",
                "vmux_layout",
                "vmux_service",
                "vmux_browser",
                "wry",
            ],
        )
    }

    fn packages(tree: &str, required: &[&str], forbidden: &[&str]) -> Result<()> {
        let mut packages = HashSet::new();
        for line in tree.lines() {
            if let Some(package) = line.split_whitespace().next() {
                packages.insert(package);
            }
        }
        for required in required {
            if !packages.contains(required) {
                return Err(format!("The consumer must exercise SDK package {required}").into());
            }
        }
        for forbidden in forbidden {
            if packages.contains(forbidden) {
                return Err(format!(
                    "Framework consumer links native application dependency {forbidden}"
                )
                .into());
            }
        }
        Ok(())
    }
}
