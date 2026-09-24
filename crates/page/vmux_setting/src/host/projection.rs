use bevy::prelude::*;
use serde_json::{Map, Value};
use std::collections::HashSet;
use vmux_core::PageMetadata;
use vmux_core::page::PageReady;
use vmux_ui::i18n::{Locale, TranslationValue};

use crate::AppSettings;
use crate::event::{CurrentUpdateCheckStatus, UpdateCheckStatus};
use crate::schema::{FieldSpec, SectionSpec, SettingsSchema, WidgetKind};
use crate::state::{
    SettingsBindingRow, SettingsRenderField, SettingsRenderFieldId, SettingsRenderFieldKind,
    SettingsRenderItem, SettingsRenderItemId, SettingsSection, SettingsUiState,
};

use super::state::Settings;

pub(super) struct ProjectionPlugin;

impl Plugin for ProjectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentUpdateCheckStatus>()
            .add_plugins(vmux_core::host::UiStatePlugin::<SettingsUiState>::default())
            .add_systems(
                Update,
                (
                    localize_settings_metadata,
                    project_settings_schema,
                    project_settings_render_fields,
                    publish_settings_ui_state,
                )
                    .chain(),
            );
    }
}

#[derive(Component, Default)]
pub(super) struct SettingsSchemaProjection(SettingsSchema);

#[derive(Component, Default)]
pub(super) struct SettingsRenderProjection(SettingsUiState);

fn localize_settings_metadata(
    settings: Res<AppSettings>,
    mut views: Query<&mut PageMetadata, With<Settings>>,
) {
    let locale = Locale::requested(Some(&settings.appearance.locale));
    let title = locale.translate("settings-title");
    for mut metadata in &mut views {
        if metadata.title != title {
            metadata.title.clone_from(&title);
        }
    }
}

fn project_settings_schema(
    settings: Res<AppSettings>,
    mut views: Query<&mut SettingsSchemaProjection, With<Settings>>,
) {
    let mut next = None;
    for mut projection in &mut views {
        if !settings.is_changed() && !projection.0.sections.is_empty() {
            continue;
        }
        let schema = next.get_or_insert_with(|| {
            SettingsSchema::localized(&Locale::requested(Some(&settings.appearance.locale)))
        });
        if projection.0 != *schema {
            projection.0.clone_from(schema);
        }
    }
}

fn project_settings_render_fields(
    settings: Res<AppSettings>,
    status: Res<CurrentUpdateCheckStatus>,
    mut views: Query<
        (Ref<SettingsSchemaProjection>, &mut SettingsRenderProjection),
        With<Settings>,
    >,
) {
    let locale = Locale::requested(Some(&settings.appearance.locale));
    for (schema, mut projection) in &mut views {
        if !settings.is_changed()
            && !status.is_changed()
            && !schema.is_changed()
            && !projection.0.sections.is_empty()
        {
            continue;
        }
        let next = SettingsUiState::projected(&settings, &schema.0, &status.0, &locale);
        if projection.0 != next {
            projection.0 = next;
        }
    }
}

fn publish_settings_ui_state(
    views: Query<(Entity, Ref<PageReady>, Ref<SettingsRenderProjection>), With<Settings>>,
    mut commands: Commands,
) {
    for (entity, ready, projection) in &views {
        if !ready.is_changed() && !projection.is_changed() {
            continue;
        }
        commands.trigger(
            vmux_core::host::UiStateWrite::<SettingsUiState>::from_event(entity, &projection.0),
        );
    }
}

impl SettingsUiState {
    fn projected(
        settings: &AppSettings,
        schema: &SettingsSchema,
        status: &UpdateCheckStatus,
        locale: &Locale,
    ) -> Self {
        let Ok(Value::Object(settings)) = serde_json::to_value(settings) else {
            return Self::default();
        };
        let mut render = SettingsRenderBuilder::default();
        let sections = SettingsSection::projected(&settings, schema, status, locale, &mut render);
        Self {
            sections,
            fields: render.fields,
            items: render.items,
        }
    }
}

#[derive(Default)]
struct SettingsRenderBuilder {
    fields: Vec<SettingsRenderField>,
    items: Vec<SettingsRenderItem>,
}

impl SettingsSection {
    fn projected(
        settings: &Map<String, Value>,
        schema: &SettingsSchema,
        status: &UpdateCheckStatus,
        locale: &Locale,
        render: &mut SettingsRenderBuilder,
    ) -> Vec<Self> {
        let mut sections = Vec::new();
        let mut consumed = HashSet::new();

        for spec in &schema.sections {
            let value = if spec.synthetic_keys.is_empty() {
                let Some(value) = settings.get(&spec.root_path) else {
                    continue;
                };
                consumed.insert(spec.root_path.clone());
                value.clone()
            } else {
                let mut object = Map::new();
                for key in &spec.synthetic_keys {
                    let Some(value) = settings.get(key) else {
                        continue;
                    };
                    object.insert(key.clone(), value.clone());
                    consumed.insert(key.clone());
                }
                if object.is_empty() {
                    continue;
                }
                Value::Object(object)
            };
            let search_text = SettingsSearchText::section(spec, &value, schema);
            let mut visible = value;
            if spec.id == "general"
                && matches!(status, UpdateCheckStatus::Unavailable)
                && let Some(object) = visible.as_object_mut()
            {
                object.remove("auto_update");
                object.remove("update_channel");
            }
            let mut fields = render.project_fields(&visible, &spec.root_path, schema, locale);
            if spec.id == "general" {
                fields.push(render.push_field(SettingsRenderField::update_check(status, locale)));
            }
            sections.push(Self {
                id: spec.id.clone(),
                title: spec.title.clone(),
                description: spec.description.clone(),
                search_text,
                fields,
            });
        }

        let mut extra_scalars = Map::new();
        for (key, value) in settings {
            if consumed.contains(key) {
                continue;
            }
            if value.is_object() {
                let spec = SectionSpec {
                    id: key.clone(),
                    title: SettingsRenderField::title(key),
                    description: None,
                    synthetic_keys: Vec::new(),
                    root_path: key.clone(),
                };
                sections.push(Self {
                    id: spec.id.clone(),
                    title: spec.title.clone(),
                    description: None,
                    search_text: SettingsSearchText::section(&spec, value, schema),
                    fields: render.project_fields(value, key, schema, locale),
                });
            } else {
                extra_scalars.insert(key.clone(), value.clone());
            }
        }

        if !extra_scalars.is_empty() {
            let value = Value::Object(extra_scalars);
            let spec = SectionSpec {
                id: "general-extra".to_string(),
                title: locale.translate("settings-other"),
                description: None,
                synthetic_keys: Vec::new(),
                root_path: String::new(),
            };
            let section = Self {
                id: spec.id.clone(),
                title: spec.title.clone(),
                description: None,
                search_text: SettingsSearchText::section(&spec, &value, schema),
                fields: render.project_fields(&value, "", schema, locale),
            };
            if sections.iter().any(|section| section.id == "general") {
                sections.push(section);
            } else {
                sections.insert(0, section);
            }
        }

        sections
    }
}

impl SettingsRenderBuilder {
    fn project_fields(
        &mut self,
        value: &Value,
        parent_path: &str,
        schema: &SettingsSchema,
        locale: &Locale,
    ) -> Vec<SettingsRenderFieldId> {
        let Some(object) = value.as_object() else {
            return Vec::new();
        };
        let mut fields = Vec::new();
        for key in schema.ordered_keys(object, parent_path) {
            let Some(field) = self.project_field(&key, &object[&key], parent_path, schema, locale)
            else {
                continue;
            };
            fields.push(field);
        }
        fields
    }

    fn project_field(
        &mut self,
        name: &str,
        value: &Value,
        parent_path: &str,
        schema: &SettingsSchema,
        locale: &Locale,
    ) -> Option<SettingsRenderFieldId> {
        let path = match parent_path.is_empty() {
            true => name.to_string(),
            false => format!("{parent_path}.{name}"),
        };
        let spec = schema.field(&path).cloned().unwrap_or_default();
        if spec.omit {
            return None;
        }
        let label = spec
            .label
            .clone()
            .unwrap_or_else(|| SettingsRenderField::title(name));
        let hint = spec.hint.clone();
        let kind = if let Some(widget) = spec.widget {
            match widget {
                WidgetKind::Select => SettingsRenderFieldKind::Select {
                    value: value.as_str().unwrap_or_default().to_string(),
                    options: spec.options,
                },
                WidgetKind::LeaderKbd => SettingsRenderFieldKind::Chord {
                    text: KeyChordText::combo(value, locale),
                },
                WidgetKind::BindingsList => {
                    let mut rows = Vec::new();
                    if let Some(bindings) = value.as_array() {
                        for (index, binding) in bindings.iter().enumerate() {
                            rows.push(SettingsBindingRow::projected(index, binding, locale));
                        }
                    }
                    SettingsRenderFieldKind::Bindings { rows }
                }
            }
        } else {
            match value {
                Value::Bool(value) => SettingsRenderFieldKind::Toggle { value: *value },
                Value::Number(value) if value.is_u64() => SettingsRenderFieldKind::Integer {
                    value: value.as_u64().unwrap_or(0),
                },
                Value::Number(value) => SettingsRenderFieldKind::Number {
                    value: value.as_f64().unwrap_or(0.0),
                    step: spec.step.unwrap_or(1.0),
                },
                Value::String(value) => SettingsRenderFieldKind::Text {
                    value: value.clone(),
                    placeholder: spec.placeholder,
                },
                Value::Object(_) => SettingsRenderFieldKind::Group {
                    fields: self.project_fields(value, &path, schema, locale),
                },
                Value::Array(items) => SettingsRenderFieldKind::Array {
                    items: self.project_items(items, &path, schema, locale),
                },
                Value::Null => return None,
            }
        };
        Some(self.push_field(SettingsRenderField {
            path,
            label,
            hint,
            kind,
        }))
    }

    fn push_field(&mut self, field: SettingsRenderField) -> SettingsRenderFieldId {
        let id = SettingsRenderFieldId(self.fields.len() as u32);
        self.fields.push(field);
        id
    }

    fn push_item(&mut self, item: SettingsRenderItem) -> SettingsRenderItemId {
        let id = SettingsRenderItemId(self.items.len() as u32);
        self.items.push(item);
        id
    }
}

impl SettingsRenderField {
    fn update_check(status: &UpdateCheckStatus, locale: &Locale) -> Self {
        let (button_label, hint, disabled) = match status {
            UpdateCheckStatus::Idle => (
                locale.translate("settings-check-updates"),
                locale.translate("settings-check-updates-hint"),
                false,
            ),
            UpdateCheckStatus::Unavailable => (
                locale.translate("settings-update-unavailable"),
                locale.translate("settings-update-unavailable-hint"),
                true,
            ),
            UpdateCheckStatus::Checking => (
                locale.translate("settings-update-checking"),
                locale.translate("settings-update-checking-hint"),
                true,
            ),
            UpdateCheckStatus::UpToDate => (
                locale.translate("settings-update-check-again"),
                locale.translate("settings-update-current"),
                false,
            ),
            UpdateCheckStatus::Downloading { version } => (
                locale.translate("settings-update-downloading"),
                locale.translate_with(
                    "settings-update-downloading-hint",
                    &[("version", TranslationValue::String(version))],
                ),
                true,
            ),
            UpdateCheckStatus::Installing { version } => (
                locale.translate("settings-update-installing"),
                locale.translate_with(
                    "settings-update-installing-hint",
                    &[("version", TranslationValue::String(version))],
                ),
                true,
            ),
            UpdateCheckStatus::Ready { version } => (
                locale.translate("settings-update-ready"),
                locale.translate_with(
                    "settings-update-ready-hint",
                    &[("version", TranslationValue::String(version))],
                ),
                true,
            ),
            UpdateCheckStatus::Failed => (
                locale.translate("settings-update-try-again"),
                locale.translate("settings-update-failed"),
                false,
            ),
        };
        Self {
            path: "software-update".to_string(),
            label: locale.translate("settings-software-update"),
            hint: Some(hint),
            kind: SettingsRenderFieldKind::UpdateCheck {
                button_label,
                disabled,
            },
        }
    }

    fn title(value: &str) -> String {
        let mut title = String::with_capacity(value.len());
        let mut next_upper = true;
        for character in value.chars() {
            if character == '_' || character == '-' {
                title.push(' ');
                next_upper = true;
            } else if next_upper {
                title.extend(character.to_uppercase());
                next_upper = false;
            } else {
                title.push(character);
            }
        }
        title
    }
}

impl SettingsRenderBuilder {
    fn project_items(
        &mut self,
        items: &[Value],
        parent_path: &str,
        schema: &SettingsSchema,
        locale: &Locale,
    ) -> Vec<SettingsRenderItemId> {
        if !items.iter().all(Value::is_object) {
            let mut projected = Vec::with_capacity(items.len());
            for item in items {
                projected.push(self.push_item(SettingsRenderItem::Value {
                    text: item.to_string(),
                }));
            }
            return projected;
        }
        let mut projected = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let title = item
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    locale.translate_with(
                        "settings-item-number",
                        &[("number", TranslationValue::Number((index + 1) as i64))],
                    )
                });
            let fields =
                self.project_fields(item, &format!("{parent_path}[{index}]"), schema, locale);
            projected.push(self.push_item(SettingsRenderItem::Object { title, fields }));
        }
        projected
    }
}

impl SettingsBindingRow {
    fn projected(index: usize, binding: &Value, locale: &Locale) -> Self {
        let command = binding
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("(unknown)")
            .to_string();
        let chord = binding
            .get("binding")
            .map(|binding| KeyChordText::binding(binding, locale))
            .unwrap_or_else(|| "(none)".to_string());
        let edit_path = binding.get("binding").and_then(|binding| {
            if binding.get("Direct").is_some() {
                Some(format!("shortcuts.bindings[{index}].binding.Direct"))
            } else if binding.get("Leader").is_some() {
                Some(format!("shortcuts.bindings[{index}].binding.Leader"))
            } else {
                None
            }
        });
        Self {
            command,
            chord,
            edit_path,
        }
    }
}

impl SettingsSchema {
    fn ordered_keys(&self, object: &Map<String, Value>, parent_path: &str) -> Vec<String> {
        let order = self
            .field(parent_path)
            .map(|field| field.order.as_slice())
            .unwrap_or_default();
        let mut keys = Vec::new();
        for key in order {
            if object.contains_key(key) {
                keys.push(key.clone());
            }
        }
        let mut rest = Vec::new();
        for key in object.keys() {
            if !order.iter().any(|ordered| ordered == key) {
                rest.push(key.clone());
            }
        }
        rest.sort();
        keys.extend(rest);
        keys
    }
}

#[derive(Default)]
struct SettingsSearchText(String);

impl SettingsSearchText {
    fn section(spec: &SectionSpec, value: &Value, schema: &SettingsSchema) -> String {
        let mut search = Self::default();
        search.push(&spec.id);
        search.push(&spec.title);
        if let Some(description) = spec.description.as_deref() {
            search.push(description);
        }
        search.push(&spec.root_path);
        search.push_value(value, &spec.root_path, schema);
        search.0
    }

    fn push_value(&mut self, value: &Value, parent_path: &str, schema: &SettingsSchema) {
        match value {
            Value::Object(object) => {
                for (key, value) in object {
                    let path = match parent_path.is_empty() {
                        true => key.clone(),
                        false => format!("{parent_path}.{key}"),
                    };
                    self.push(key);
                    self.push(&path);
                    if let Some(spec) = schema.field(&path) {
                        self.push_spec(spec);
                    }
                    self.push_value(value, &path, schema);
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.push_value(item, parent_path, schema);
                }
            }
            Value::String(value) => self.push(value),
            Value::Number(value) => self.push(&value.to_string()),
            Value::Bool(value) => self.push(&value.to_string()),
            Value::Null => {}
        }
    }

    fn push_spec(&mut self, spec: &FieldSpec) {
        if let Some(label) = spec.label.as_deref() {
            self.push(label);
        }
        if let Some(description) = spec.description.as_deref() {
            self.push(description);
        }
        if let Some(hint) = spec.hint.as_deref() {
            self.push(hint);
        }
        if let Some(placeholder) = spec.placeholder.as_deref() {
            self.push(placeholder);
        }
        for option in &spec.options {
            self.push(&option.value);
            self.push(&option.label);
        }
    }

    fn push(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }
        if !self.0.is_empty() {
            self.0.push('\n');
        }
        self.0.push_str(&value.to_lowercase());
    }
}

struct KeyChordText;

impl KeyChordText {
    fn combo(combo: &Value, locale: &Locale) -> String {
        let mut parts = Vec::new();
        if combo.get("ctrl").and_then(Value::as_bool).unwrap_or(false) {
            parts.push("Ctrl".to_string());
        }
        if combo
            .get("super_key")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            parts.push("⌘".to_string());
        }
        if combo.get("alt").and_then(Value::as_bool).unwrap_or(false) {
            parts.push("Alt".to_string());
        }
        if combo.get("shift").and_then(Value::as_bool).unwrap_or(false) {
            parts.push("Shift".to_string());
        }
        if let Some(key) = combo.get("key").and_then(Value::as_str) {
            let key = match key {
                "ArrowLeft" => "←".to_string(),
                "ArrowRight" => "→".to_string(),
                "ArrowUp" => "↑".to_string(),
                "ArrowDown" => "↓".to_string(),
                "Tab" => "Tab".to_string(),
                "Enter" => "↵".to_string(),
                "Space" => "␣".to_string(),
                key if key.len() == 1 => key.to_uppercase(),
                key => key.to_string(),
            };
            parts.push(key);
        }
        if parts.is_empty() {
            locale.translate("settings-none")
        } else {
            parts.join(" + ")
        }
    }

    fn binding(binding: &Value, locale: &Locale) -> String {
        if let Some(direct) = binding.get("Direct") {
            return Self::combo(direct, locale);
        }
        if let Some(leader) = binding.get("Leader") {
            return format!(
                "{} → {}",
                locale.translate("schema-leader"),
                Self::combo(leader, locale)
            );
        }
        if let Some(chord) = binding.get("Chord").and_then(Value::as_array) {
            let mut combinations = Vec::with_capacity(chord.len());
            for combo in chord {
                combinations.push(Self::combo(combo, locale));
            }
            return combinations.join(" → ");
        }
        binding.to_string()
    }
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use bevy_cef::prelude::{BinHostEmitEvent, Browsers};
    use vmux_api::BinEvent;

    #[derive(Resource, Default)]
    struct Emitted(Vec<SettingsUiState>);

    fn record_settings_state(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<Emitted>) {
        if trigger.event().id() != SettingsUiState::id() {
            return;
        }
        let state =
            rkyv::from_bytes::<SettingsUiState, rkyv::rancor::Error>(trigger.event().payload())
                .unwrap();
        emitted.0.push(state);
    }

    #[test]
    fn initial_settings_state_projects_typed_fields() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ProjectionPlugin))
            .insert_resource(AppSettings::embedded())
            .init_resource::<Emitted>()
            .add_observer(record_settings_state);
        let entity = app.world_mut().spawn((Settings, PageReady)).id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);

        app.update();

        let emitted = &app.world().resource::<Emitted>().0;
        assert_eq!(emitted.len(), 1);
        let general = emitted[0]
            .sections
            .iter()
            .find(|section| section.id == "general")
            .expect("general section");
        let auto_update = general
            .fields
            .iter()
            .filter_map(|field| emitted[0].fields.get(field.0 as usize))
            .find(|field| field.path == "auto_update")
            .expect("auto update field");
        assert!(matches!(
            &auto_update.kind,
            SettingsRenderFieldKind::Toggle { .. }
        ));
        assert!(matches!(
            general
                .fields
                .last()
                .and_then(|field| emitted[0].fields.get(field.0 as usize))
                .map(|field| &field.kind),
            Some(SettingsRenderFieldKind::UpdateCheck { .. })
        ));
    }

    #[test]
    fn search_projection_includes_nested_schema_and_values() {
        let schema = SettingsSchema {
            sections: vec![SectionSpec {
                id: "agent".into(),
                title: "Agent".into(),
                description: Some("Agent behavior".into()),
                synthetic_keys: Vec::new(),
                root_path: "agent".into(),
            }],
            fields: vec![(
                "agent.acp[].command".into(),
                FieldSpec {
                    label: Some("Command".into()),
                    ..Default::default()
                },
            )],
        };
        let settings = serde_json::json!({
            "agent": {
                "acp": [
                    {
                        "command": "codex"
                    }
                ]
            }
        });
        let Value::Object(settings) = settings else {
            panic!("object");
        };

        let mut render = SettingsRenderBuilder::default();
        let sections = SettingsSection::projected(
            &settings,
            &schema,
            &UpdateCheckStatus::Idle,
            &Locale::from("en-US"),
            &mut render,
        );

        assert!(sections[0].search_text.contains("agent behavior"));
        assert!(sections[0].search_text.contains("command"));
        assert!(sections[0].search_text.contains("codex"));
    }
}
