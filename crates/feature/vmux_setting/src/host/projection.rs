use bevy::prelude::*;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};
use vmux_ecs::PageMetadata;
use vmux_ecs::page::PageReady;
use vmux_ui::i18n::{Locale, TranslationValue};

use super::runtime::SettingsManifest;
use crate::AppSettings;
use crate::event::{CurrentUpdateCheckStatus, UpdateCheckStatus};
use crate::schema::{FieldSpec, SectionSpec, SettingsSchema, WidgetKind};
use crate::state::{
    SettingsBindingRow, SettingsRenderField, SettingsRenderFieldId, SettingsRenderFieldKind,
    SettingsRenderItem, SettingsRenderItemId, SettingsSection, SettingsUiState,
};

use super::state::{Settings, SettingsViewState};

pub(super) struct ProjectionPlugin;

impl Plugin for ProjectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_status)
            .add_plugins(vmux_ecs::UiStatePlugin::<SettingsUiState>::default())
            .add_systems(
                Update,
                (localize_metadata, project_schema, project_fields, publish).chain(),
            );
    }
}

fn spawn_status(mut commands: Commands) {
    commands.spawn((
        Name::new("Update check status"),
        CurrentUpdateCheckStatus::default(),
    ));
}

#[derive(Component, Default)]
pub(super) struct SettingsRenderProjection(SettingsUiState);

impl SettingsRenderProjection {
    pub(super) fn field(&self, path: &str) -> Option<&SettingsRenderField> {
        self.0.fields.iter().find(|field| field.path == path)
    }

    fn build_state(
        settings: &AppSettings,
        schema: &SettingsSchema,
        status: &UpdateCheckStatus,
        locale: &Locale,
        view: &SettingsViewState,
    ) -> SettingsUiState {
        let Ok(Value::Object(settings)) = serde_json::to_value(settings) else {
            return SettingsUiState::default();
        };
        let mut fields = Vec::new();
        let mut items = Vec::new();
        let mut sections = Self::project_sections(
            &settings,
            schema,
            status,
            locale,
            &view.drafts,
            &mut fields,
            &mut items,
        );
        let query = view.query.trim().to_lowercase();
        if !query.is_empty() {
            sections.retain(|section| section.search_text.contains(&query));
        }
        SettingsUiState {
            query: view.query.clone(),
            sections,
            fields,
            items,
        }
    }

    fn project_sections(
        settings: &Map<String, Value>,
        schema: &SettingsSchema,
        status: &UpdateCheckStatus,
        locale: &Locale,
        drafts: &BTreeMap<String, String>,
        fields_out: &mut Vec<SettingsRenderField>,
        items_out: &mut Vec<SettingsRenderItem>,
    ) -> Vec<SettingsSection> {
        let mut sections = Vec::new();
        let mut consumed = HashSet::new();
        for spec in &schema.sections {
            if let Some(section) = Self::project_schema_section(
                spec,
                settings,
                schema,
                status,
                locale,
                drafts,
                &mut consumed,
                fields_out,
                items_out,
            ) {
                sections.push(section);
            }
        }
        Self::append_unclaimed_sections(
            settings,
            schema,
            locale,
            drafts,
            &consumed,
            &mut sections,
            fields_out,
            items_out,
        );
        sections
    }

    #[allow(clippy::too_many_arguments)]
    fn project_schema_section(
        spec: &SectionSpec,
        settings: &Map<String, Value>,
        schema: &SettingsSchema,
        status: &UpdateCheckStatus,
        locale: &Locale,
        drafts: &BTreeMap<String, String>,
        consumed: &mut HashSet<String>,
        fields_out: &mut Vec<SettingsRenderField>,
        items_out: &mut Vec<SettingsRenderItem>,
    ) -> Option<SettingsSection> {
        let value = if spec.synthetic_keys.is_empty() {
            let value = settings.get(&spec.root_path)?;
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
                return None;
            }
            Value::Object(object)
        };
        let search_text = Self::section_search_text(spec, &value, schema);
        let mut visible = value;
        if spec.id == "general"
            && matches!(status, UpdateCheckStatus::Unavailable)
            && let Some(object) = visible.as_object_mut()
        {
            object.remove("auto_update");
            object.remove("update_channel");
        }
        let mut section_fields = Self::project_object_fields(
            &visible,
            &spec.root_path,
            schema,
            locale,
            drafts,
            fields_out,
            items_out,
        );
        if spec.id == "general" {
            section_fields.push(Self::push_field(
                fields_out,
                Self::update_check_field(status, locale),
            ));
        }
        Some(SettingsSection {
            id: spec.id.clone(),
            title: spec.title.clone(),
            description: spec.description.clone(),
            search_text,
            fields: section_fields,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn append_unclaimed_sections(
        settings: &Map<String, Value>,
        schema: &SettingsSchema,
        locale: &Locale,
        drafts: &BTreeMap<String, String>,
        consumed: &HashSet<String>,
        sections: &mut Vec<SettingsSection>,
        fields_out: &mut Vec<SettingsRenderField>,
        items_out: &mut Vec<SettingsRenderItem>,
    ) {
        let mut extra_scalars = Map::new();
        for (key, value) in settings {
            if consumed.contains(key) {
                continue;
            }
            if value.is_object() {
                let spec = SectionSpec {
                    id: key.clone(),
                    title: Self::title(key),
                    description: None,
                    synthetic_keys: Vec::new(),
                    root_path: key.clone(),
                    order: 0,
                };
                sections.push(SettingsSection {
                    id: spec.id.clone(),
                    title: spec.title.clone(),
                    description: None,
                    search_text: Self::section_search_text(&spec, value, schema),
                    fields: Self::project_object_fields(
                        value, key, schema, locale, drafts, fields_out, items_out,
                    ),
                });
            } else {
                extra_scalars.insert(key.clone(), value.clone());
            }
        }
        if extra_scalars.is_empty() {
            return;
        }
        let value = Value::Object(extra_scalars);
        let spec = SectionSpec {
            id: "general-extra".to_string(),
            title: locale.translate("settings-other"),
            description: None,
            synthetic_keys: Vec::new(),
            root_path: String::new(),
            order: 0,
        };
        let section = SettingsSection {
            id: spec.id.clone(),
            title: spec.title.clone(),
            description: None,
            search_text: Self::section_search_text(&spec, &value, schema),
            fields: Self::project_object_fields(
                &value, "", schema, locale, drafts, fields_out, items_out,
            ),
        };
        if sections.iter().any(|section| section.id == "general") {
            sections.push(section);
        } else {
            sections.insert(0, section);
        }
    }

    fn project_object_fields(
        value: &Value,
        parent_path: &str,
        schema: &SettingsSchema,
        locale: &Locale,
        drafts: &BTreeMap<String, String>,
        fields_out: &mut Vec<SettingsRenderField>,
        items_out: &mut Vec<SettingsRenderItem>,
    ) -> Vec<SettingsRenderFieldId> {
        let Some(object) = value.as_object() else {
            return Vec::new();
        };
        let mut fields = Vec::new();
        for key in schema.ordered_keys(object, parent_path) {
            let Some(field) = Self::project_field(
                &key,
                &object[&key],
                parent_path,
                schema,
                locale,
                drafts,
                fields_out,
                items_out,
            ) else {
                continue;
            };
            fields.push(field);
        }
        fields
    }

    #[allow(clippy::too_many_arguments)]
    fn project_field(
        name: &str,
        value: &Value,
        parent_path: &str,
        schema: &SettingsSchema,
        locale: &Locale,
        drafts: &BTreeMap<String, String>,
        fields_out: &mut Vec<SettingsRenderField>,
        items_out: &mut Vec<SettingsRenderItem>,
    ) -> Option<SettingsRenderFieldId> {
        let path = match parent_path.is_empty() {
            true => name.to_string(),
            false => format!("{parent_path}.{name}"),
        };
        let spec = schema.field(&path).cloned().unwrap_or_default();
        if spec.omit {
            return None;
        }
        let label = spec.label.clone().unwrap_or_else(|| Self::title(name));
        let hint = spec.hint.clone();
        let kind = if let Some(widget) = spec.widget {
            match widget {
                WidgetKind::Select => SettingsRenderFieldKind::Select {
                    value: value.as_str().unwrap_or_default().to_string(),
                    options: spec.options,
                },
                WidgetKind::LeaderKbd => SettingsRenderFieldKind::Chord {
                    text: Self::chord_combo(value, locale),
                },
                WidgetKind::BindingsList => {
                    let mut rows = Vec::new();
                    if let Some(bindings) = value.as_array() {
                        for (index, binding) in bindings.iter().enumerate() {
                            rows.push(Self::project_binding_row(index, binding, locale));
                        }
                    }
                    SettingsRenderFieldKind::Bindings { rows }
                }
            }
        } else {
            match value {
                Value::Bool(value) => SettingsRenderFieldKind::Toggle { value: *value },
                Value::Number(value) if value.is_u64() => SettingsRenderFieldKind::Integer {
                    value: drafts
                        .get(&path)
                        .cloned()
                        .unwrap_or_else(|| value.to_string()),
                },
                Value::Number(value) => SettingsRenderFieldKind::Number {
                    value: drafts
                        .get(&path)
                        .cloned()
                        .unwrap_or_else(|| value.to_string()),
                    step: spec.step.unwrap_or(1.0),
                },
                Value::String(value) => SettingsRenderFieldKind::Text {
                    value: drafts.get(&path).cloned().unwrap_or_else(|| value.clone()),
                    placeholder: spec.placeholder,
                },
                Value::Object(_) => SettingsRenderFieldKind::Group {
                    fields: Self::project_object_fields(
                        value, &path, schema, locale, drafts, fields_out, items_out,
                    ),
                },
                Value::Array(items) => SettingsRenderFieldKind::Array {
                    items: Self::project_items(
                        items, &path, schema, locale, drafts, fields_out, items_out,
                    ),
                },
                Value::Null => return None,
            }
        };
        Some(Self::push_field(
            fields_out,
            SettingsRenderField {
                path,
                label,
                hint,
                kind,
            },
        ))
    }

    fn project_items(
        items: &[Value],
        parent_path: &str,
        schema: &SettingsSchema,
        locale: &Locale,
        drafts: &BTreeMap<String, String>,
        fields_out: &mut Vec<SettingsRenderField>,
        items_out: &mut Vec<SettingsRenderItem>,
    ) -> Vec<SettingsRenderItemId> {
        if !items.iter().all(Value::is_object) {
            let mut projected = Vec::with_capacity(items.len());
            for item in items {
                projected.push(Self::push_item(
                    items_out,
                    SettingsRenderItem::Value {
                        text: item.to_string(),
                    },
                ));
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
            let fields = Self::project_object_fields(
                item,
                &format!("{parent_path}[{index}]"),
                schema,
                locale,
                drafts,
                fields_out,
                items_out,
            );
            projected.push(Self::push_item(
                items_out,
                SettingsRenderItem::Object { title, fields },
            ));
        }
        projected
    }

    fn push_field(
        fields_out: &mut Vec<SettingsRenderField>,
        field: SettingsRenderField,
    ) -> SettingsRenderFieldId {
        let id = SettingsRenderFieldId(fields_out.len() as u32);
        fields_out.push(field);
        id
    }

    fn push_item(
        items_out: &mut Vec<SettingsRenderItem>,
        item: SettingsRenderItem,
    ) -> SettingsRenderItemId {
        let id = SettingsRenderItemId(items_out.len() as u32);
        items_out.push(item);
        id
    }

    fn update_check_field(status: &UpdateCheckStatus, locale: &Locale) -> SettingsRenderField {
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
        SettingsRenderField {
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

    fn project_binding_row(index: usize, binding: &Value, locale: &Locale) -> SettingsBindingRow {
        let command = binding
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("(unknown)")
            .to_string();
        let chord = binding
            .get("binding")
            .map(|binding| Self::chord_binding(binding, locale))
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
        SettingsBindingRow {
            command,
            chord,
            edit_path,
        }
    }

    fn section_search_text(spec: &SectionSpec, value: &Value, schema: &SettingsSchema) -> String {
        let mut search = String::new();
        Self::push_search(&mut search, &spec.id);
        Self::push_search(&mut search, &spec.title);
        if let Some(description) = spec.description.as_deref() {
            Self::push_search(&mut search, description);
        }
        Self::push_search(&mut search, &spec.root_path);
        Self::push_search_value(&mut search, value, &spec.root_path, schema);
        search
    }

    fn push_search_value(
        search: &mut String,
        value: &Value,
        parent_path: &str,
        schema: &SettingsSchema,
    ) {
        match value {
            Value::Object(object) => {
                for (key, value) in object {
                    let path = match parent_path.is_empty() {
                        true => key.clone(),
                        false => format!("{parent_path}.{key}"),
                    };
                    Self::push_search(search, key);
                    Self::push_search(search, &path);
                    if let Some(spec) = schema.field(&path) {
                        Self::push_search_spec(search, spec);
                    }
                    Self::push_search_value(search, value, &path, schema);
                }
            }
            Value::Array(items) => {
                for item in items {
                    Self::push_search_value(search, item, parent_path, schema);
                }
            }
            Value::String(value) => Self::push_search(search, value),
            Value::Number(value) => Self::push_search(search, &value.to_string()),
            Value::Bool(value) => Self::push_search(search, &value.to_string()),
            Value::Null => {}
        }
    }

    fn push_search_spec(search: &mut String, spec: &FieldSpec) {
        if let Some(label) = spec.label.as_deref() {
            Self::push_search(search, label);
        }
        if let Some(description) = spec.description.as_deref() {
            Self::push_search(search, description);
        }
        if let Some(hint) = spec.hint.as_deref() {
            Self::push_search(search, hint);
        }
        if let Some(placeholder) = spec.placeholder.as_deref() {
            Self::push_search(search, placeholder);
        }
        for option in &spec.options {
            Self::push_search(search, &option.value);
            Self::push_search(search, &option.label);
        }
    }

    fn push_search(search: &mut String, value: &str) {
        if value.is_empty() {
            return;
        }
        if !search.is_empty() {
            search.push('\n');
        }
        search.push_str(&value.to_lowercase());
    }

    fn chord_combo(combo: &Value, locale: &Locale) -> String {
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

    fn chord_binding(binding: &Value, locale: &Locale) -> String {
        if let Some(direct) = binding.get("Direct") {
            return Self::chord_combo(direct, locale);
        }
        if let Some(leader) = binding.get("Leader") {
            return format!(
                "{} → {}",
                locale.translate("schema-leader"),
                Self::chord_combo(leader, locale)
            );
        }
        if let Some(chord) = binding.get("Chord").and_then(Value::as_array) {
            let mut combinations = Vec::with_capacity(chord.len());
            for combo in chord {
                combinations.push(Self::chord_combo(combo, locale));
            }
            return combinations.join(" → ");
        }
        binding.to_string()
    }
}

fn localize_metadata(
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

fn project_schema(
    settings: Res<AppSettings>,
    manifests: Query<&SettingsManifest, Without<Settings>>,
    changed_manifests: Query<(), Changed<SettingsManifest>>,
    profile: vmux_ecs::profile::CurrentProfile,
    mut views: Query<&mut SettingsSchema, With<Settings>>,
) {
    let added = views.iter_mut().any(|schema| schema.is_added());
    if !settings.is_changed() && changed_manifests.is_empty() && !added {
        return;
    }
    let locale = Locale::requested(Some(&settings.appearance.locale));
    let mut schema = SettingsSchema::default();
    for manifest in &manifests {
        schema.sections.extend(manifest.sections.iter().cloned());
        schema.fields.extend(manifest.fields.iter().cloned());
    }
    schema.sections.sort_by_key(|section| section.order);
    let locales = profile.paths().map(|paths| paths.config().join("locales"));
    schema.localize(&locale, locales.as_deref());
    for mut current in &mut views {
        if *current != schema {
            current.clone_from(&schema);
        }
    }
}

fn project_fields(
    settings: Res<AppSettings>,
    status: Single<Ref<CurrentUpdateCheckStatus>>,
    mut views: Query<
        (
            Ref<SettingsSchema>,
            Ref<SettingsViewState>,
            &mut SettingsRenderProjection,
        ),
        With<Settings>,
    >,
) {
    let locale = Locale::requested(Some(&settings.appearance.locale));
    for (schema, view, mut projection) in &mut views {
        if !settings.is_changed()
            && !status.is_changed()
            && !schema.is_changed()
            && !view.is_changed()
            && !projection.0.sections.is_empty()
        {
            continue;
        }
        let next =
            SettingsRenderProjection::build_state(&settings, &schema, &status.0, &locale, &view);
        if projection.0 != next {
            projection.0 = next;
        }
    }
}

fn publish(
    views: Query<(Entity, Ref<PageReady>, Ref<SettingsRenderProjection>), With<Settings>>,
    mut commands: Commands,
) {
    for (entity, ready, projection) in &views {
        if !ready.is_changed() && !projection.is_changed() {
            continue;
        }
        commands.trigger(vmux_ecs::UiStateWrite::<SettingsUiState>::from_event(
            entity,
            &projection.0,
        ));
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

#[cfg(test)]
mod projection_tests {
    use super::*;
    use bevy_cef::prelude::{BinHostEmitEvent, Browsers};
    use vmux_api::{BinEvent, UiState as UiStateContract, UiStateFrame};

    #[derive(Resource, Default)]
    struct Emitted(Vec<SettingsUiState>);

    fn record_settings_state(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<Emitted>) {
        if trigger.event().id() != SettingsUiState::id() {
            return;
        }
        let frame = rkyv::from_bytes::<
            UiStateFrame<SettingsUiState, <SettingsUiState as UiStateContract>::Update>,
            rkyv::rancor::Error,
        >(trigger.event().payload())
        .unwrap();
        let state = match frame {
            UiStateFrame::Snapshot(state) => state,
            UiStateFrame::Updates(updates) => {
                SettingsUiState::from_updates(emitted.0.last().cloned(), updates)
            }
        };
        emitted.0.push(state);
        trigger.event().mark_delivered();
    }

    #[test]
    fn initial_settings_state_projects_typed_fields() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ProjectionPlugin))
            .insert_resource(AppSettings::default())
            .init_resource::<Emitted>()
            .add_observer(record_settings_state);
        let feature =
            vmux_feature::manifest::FeatureManifest::parse(include_str!("../feature.ron"));
        let settings = feature
            .settings::<SettingsManifest>()
            .unwrap()
            .expect("settings manifest");
        app.world_mut().spawn((feature, settings));
        let entity = app
            .world_mut()
            .spawn((
                Settings,
                PageReady,
                PageMetadata {
                    url: "vmux://settings/".to_string(),
                    ..Default::default()
                },
                bevy_cef::prelude::BinIpcSurfaceGeneration(1),
                bevy_cef::prelude::BinIpcNavigationRevision(1),
                bevy_cef::prelude::BinIpcEpoch(1),
            ))
            .id();
        app.world_mut().spawn(vmux_api::UiEventPermissions {
            url: "vmux://settings/",
            owns_subtree: false,
            permissions: &[SettingsUiState::PERMISSION],
        });
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
    fn feature_manifests_spawn_settings_components_and_compose_the_page_schema() {
        let manifests = [
            include_str!("../feature.ron"),
            include_str!("../../../vmux_acp/src/feature.ron"),
            include_str!("../../../vmux_editor/src/feature.ron"),
            include_str!("../../../vmux_input/src/feature.ron"),
            include_str!("../../../vmux_layout/src/feature.ron"),
            include_str!("../../../vmux_shortcut/src/feature.ron"),
            include_str!("../../../vmux_space/src/feature.ron"),
            include_str!("../../../vmux_terminal/src/feature.ron"),
            include_str!("../../../../util/vmux_browser/src/feature.ron"),
        ];
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ProjectionPlugin))
            .insert_resource(AppSettings::default());
        let page = app.world_mut().spawn(Settings).id();
        for source in manifests {
            let feature = vmux_feature::manifest::FeatureManifest::parse(source);
            let settings = feature
                .settings::<SettingsManifest>()
                .unwrap()
                .expect("settings manifest");
            app.world_mut().spawn((feature, settings));
        }

        app.update();

        let mut loaded = app.world_mut().query::<&SettingsManifest>();
        assert_eq!(loaded.iter(app.world()).count(), manifests.len());
        let schema = app
            .world()
            .get::<SettingsSchema>(page)
            .expect("settings page schema");
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
    fn search_projection_includes_nested_schema_and_values() {
        let schema = SettingsSchema {
            sections: vec![SectionSpec {
                id: "agent".into(),
                title: "Agent".into(),
                description: Some("Agent behavior".into()),
                synthetic_keys: Vec::new(),
                root_path: "agent".into(),
                order: 0,
            }],
            fields: vec![FieldSpec {
                path: "agent.acp[].command".into(),
                label: Some("Command".into()),
                ..Default::default()
            }],
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

        let mut fields_out = Vec::new();
        let mut items_out = Vec::new();
        let sections = SettingsRenderProjection::project_sections(
            &settings,
            &schema,
            &UpdateCheckStatus::Idle,
            &Locale::from("en-US"),
            &BTreeMap::new(),
            &mut fields_out,
            &mut items_out,
        );

        assert!(sections[0].search_text.contains("agent behavior"));
        assert!(sections[0].search_text.contains("command"));
        assert!(sections[0].search_text.contains("codex"));
    }
}
