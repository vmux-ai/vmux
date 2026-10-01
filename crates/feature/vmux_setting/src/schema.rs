use bevy::prelude::*;
use serde::Deserialize;
use vmux_core::host::manifest::FeatureManifest;
use vmux_ui::i18n::Locale;

use crate::state::SettingsSelectOption;

#[derive(Component, Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SettingsSchema {
    #[serde(default)]
    pub sections: Vec<SectionSpec>,
    #[serde(default)]
    pub fields: Vec<FieldSpec>,
}

impl SettingsSchema {
    pub fn from_manifests<'a>(
        manifests: impl IntoIterator<Item = &'a FeatureManifest>,
        locale: &Locale,
    ) -> Result<Self, String> {
        let mut schema = Self::default();
        for manifest in manifests {
            let Some(mut contribution) = manifest.settings::<Self>()? else {
                continue;
            };
            schema.sections.append(&mut contribution.sections);
            schema.fields.append(&mut contribution.fields);
        }
        schema.sections.sort_by_key(|section| section.order);
        schema.localize(locale);
        Ok(schema)
    }

    pub fn field(&self, path: &str) -> Option<&FieldSpec> {
        if let Some(field) = self.fields.iter().find(|field| field.path == path) {
            return Some(field);
        }
        let path = SettingsPath::from(path);
        self.fields
            .iter()
            .find(|field| SettingsPath::from(field.path.as_str()).matches(&path))
    }

    fn localize(&mut self, locale: &Locale) {
        let directory = vmux_core::profile::ProfilePaths::current()
            .config()
            .join("locales");
        let tag = locale.as_str();
        for tag in [tag, tag.split('-').next().unwrap_or(tag)] {
            let Ok(source) = std::fs::read_to_string(directory.join(format!("{tag}.ftl"))) else {
                continue;
            };
            let _ = locale.register_catalog(&source);
            break;
        }
        for section in &mut self.sections {
            section.title = locale.translate(&section.title);
            if let Some(description) = &mut section.description {
                *description = locale.translate(description);
            }
        }
        for field in &mut self.fields {
            Self::localize_text(&mut field.label, locale);
            Self::localize_text(&mut field.description, locale);
            Self::localize_text(&mut field.hint, locale);
            for option in &mut field.options {
                option.label = locale.translate(&option.label);
            }
            if field.options_source == Some(SettingsOptionsSource::Locale) {
                field.options = vec![SettingsSelectOption {
                    value: "system".to_string(),
                    label: locale.translate("schema-system"),
                }];
                for available in Locale::available() {
                    field.options.push(SettingsSelectOption {
                        value: available.as_str().to_string(),
                        label: available.name().to_string(),
                    });
                }
            }
        }
    }

    fn localize_text(value: &mut Option<String>, locale: &Locale) {
        if let Some(value) = value {
            *value = locale.translate(value);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SectionSpec {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub synthetic_keys: Vec<String>,
    pub root_path: String,
    #[serde(default)]
    pub order: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FieldSpec {
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub widget: Option<WidgetKind>,
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default)]
    pub omit: bool,
    #[serde(default)]
    pub step: Option<f64>,
    #[serde(default)]
    pub options: Vec<SettingsSelectOption>,
    #[serde(default)]
    pub(super) options_source: Option<SettingsOptionsSource>,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub(super) enum WidgetKind {
    LeaderKbd,
    BindingsList,
    Select,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub(super) enum SettingsOptionsSource {
    Locale,
}

struct SettingsPath(String);

impl SettingsPath {
    fn matches(&self, path: &Self) -> bool {
        let pattern_segments = self.0.split('.').collect::<Vec<_>>();
        let path_segments = path.0.split('.').collect::<Vec<_>>();
        pattern_segments.len() == path_segments.len()
            && pattern_segments
                .iter()
                .zip(path_segments)
                .all(|(pattern, segment)| *pattern == "*" || *pattern == segment)
    }
}

impl From<&str> for SettingsPath {
    fn from(path: &str) -> Self {
        let mut normalized = String::with_capacity(path.len());
        let mut chars = path.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '[' {
                normalized.push(ch);
                continue;
            }
            let mut digits = String::new();
            while chars.peek().is_some_and(char::is_ascii_digit) {
                digits.push(chars.next().unwrap());
            }
            if !digits.is_empty() && chars.next_if_eq(&']').is_some() {
                normalized.push_str("[]");
            } else {
                normalized.push('[');
                normalized.push_str(&digits);
            }
        }
        Self(normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFESTS: [&str; 9] = [
        include_str!("feature.ron"),
        include_str!("../../vmux_agent/src/feature.ron"),
        include_str!("../../vmux_editor/src/feature.ron"),
        include_str!("../../vmux_input/src/feature.ron"),
        include_str!("../../vmux_layout/src/feature.ron"),
        include_str!("../../vmux_shortcut/src/feature.ron"),
        include_str!("../../vmux_space/src/feature.ron"),
        include_str!("../../vmux_terminal/src/feature.ron"),
        include_str!("../../../util/vmux_browser/src/feature.ron"),
    ];

    #[test]
    fn feature_manifests_compose_the_settings_schema() {
        let manifests = MANIFESTS
            .into_iter()
            .map(FeatureManifest::parse)
            .collect::<Vec<_>>();
        let schema = SettingsSchema::from_manifests(manifests.iter(), &Locale::from("en-US"))
            .expect("settings schema");
        let sections = schema
            .sections
            .iter()
            .map(|section| section.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            sections,
            [
                "general",
                "appearance",
                "layout",
                "agent",
                "shortcuts",
                "terminal",
                "browser",
                "editor",
                "recording",
                "spaces",
            ]
        );
        let language = schema.field("appearance.locale").expect("language field");
        assert_eq!(language.widget, Some(WidgetKind::Select));
        assert_eq!(language.options.first().unwrap().value, "system");
        assert_eq!(
            schema
                .field("agent.acp[0].command")
                .unwrap()
                .label
                .as_deref(),
            Some("Command")
        );
        assert_eq!(
            schema
                .field("spaces.personal.startup_dir")
                .unwrap()
                .label
                .as_deref(),
            Some("Startup directory")
        );
    }

    #[test]
    fn field_lookup_matches_array_indexes_and_dynamic_map_keys() {
        let schema = SettingsSchema {
            fields: vec![
                FieldSpec {
                    path: "agent.acp[].command".into(),
                    label: Some("Command".into()),
                    ..Default::default()
                },
                FieldSpec {
                    path: "spaces.*.startup_url".into(),
                    label: Some("Startup URL".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            schema
                .field("agent.acp[2].command")
                .unwrap()
                .label
                .as_deref(),
            Some("Command")
        );
        assert_eq!(
            schema
                .field("spaces.personal.startup_url")
                .unwrap()
                .label
                .as_deref(),
            Some("Startup URL")
        );
    }
}
