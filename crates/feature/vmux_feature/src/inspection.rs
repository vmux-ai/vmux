use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::flag::{Context, Decision, Value};
use crate::manifest::FeatureManifest;
use crate::state::{
    AssignmentRow, ContextRow, FeatureAssignmentRetireRequest, FeatureOverrideClearRequest,
    FeatureOverrideRequest, InspectionRow,
};
use crate::{Definition, DevelopmentOverride, OverridePolicy, Provider};

pub(crate) struct InspectionPlugin;

impl Plugin for InspectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            FeatureOverrideRequest,
            FeatureOverrideClearRequest,
            FeatureAssignmentRetireRequest,
        )>::default())
            .add_observer(apply)
            .add_observer(clear)
            .add_observer(retire);
    }
}

#[derive(Component)]
pub(crate) struct OverrideOutcome(pub(crate) String);

type Evaluations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Definition,
        &'static ChildOf,
        Option<&'static Decision<Value>>,
        Option<&'static DevelopmentOverride>,
    ),
>;

#[derive(SystemParam)]
pub(crate) struct Inspection<'w, 's> {
    sources: Query<'w, 's, (Entity, &'static FeatureManifest)>,
    evaluations: Evaluations<'w, 's>,
    contexts: Query<'w, 's, &'static Context>,
    providers: Query<'w, 's, &'static Name, With<Provider>>,
    policy: Res<'w, OverridePolicy>,
    now: Res<'w, crate::runtime::Now>,
    environments:
        Query<'w, 's, (Entity, &'static Name, &'static crate::cache::Binding), With<Provider>>,
    assignments: Query<'w, 's, (&'static ChildOf, &'static crate::flag::Assignment)>,
    configurations: Query<
        'w,
        's,
        (
            &'static Name,
            &'static ChildOf,
            &'static crate::flag::Configuration,
        ),
    >,
}

impl Inspection<'_, '_> {
    pub(crate) fn generations(&self, environment: &str) -> Vec<AssignmentRow> {
        let mut rows = std::collections::BTreeMap::<(String, String), AssignmentRow>::new();
        for (provider, name, binding) in &self.environments {
            if name.as_str() != environment {
                continue;
            }
            for (parent, assignment) in &self.assignments {
                if parent.parent() != binding.store || assignment.environment != environment {
                    continue;
                }
                let row = rows
                    .entry((assignment.flag.clone(), assignment.generation.clone()))
                    .or_insert_with(|| AssignmentRow {
                        key: assignment.flag.clone(),
                        generation: assignment.generation.clone(),
                        ..Default::default()
                    });
                row.subjects += 1;
                row.active |= self
                    .configurations
                    .iter()
                    .any(|(key, parent, configuration)| {
                        parent.parent() == provider
                            && key.as_str() == assignment.flag
                            && configuration.experiment_generation.as_deref()
                                == Some(assignment.generation.as_str())
                    });
            }
        }
        rows.into_values().collect()
    }

    pub(crate) fn allows_overrides(&self) -> bool {
        self.policy.allows_overrides()
    }

    pub(crate) fn rows(&self, environment: &str) -> Vec<InspectionRow> {
        let mut rows = Vec::new();
        for (source, manifest) in &self.sources {
            for (index, declaration) in manifest.flags.iter().enumerate() {
                let mut bound = false;
                for (_, definition, parent, decision, local_override) in &self.evaluations {
                    if definition.source != source
                        || definition.index != index
                        || !self
                            .providers
                            .get(parent.parent())
                            .is_ok_and(|name| name.as_str() == environment)
                    {
                        continue;
                    }
                    let context = self.contexts.get(definition.scope).ok();
                    let mut row = InspectionRow::from_declaration(declaration);
                    if let Some(context) = context {
                        row.scope = context.subject.key.clone();
                        row.scope_kind = context.subject.kind.as_str().into();
                        row.context_generation = context.generation;
                        for (attribute, value) in &context.attributes {
                            row.context.push(ContextRow {
                                attribute: format!("{attribute:?}"),
                                value: value.encode(),
                            });
                        }
                    }
                    if let Some(decision) = decision {
                        row.effective = decision.value.encode();
                        row.variant = decision.details.variant.clone().unwrap_or_default();
                        row.reason = format!("{:?}", decision.details.reason);
                        row.source = format!("{:?}", decision.details.source);
                        row.retained_revision = decision.details.retained_revision.unwrap_or(0);
                        if let Some(snapshot) = &decision.details.snapshot {
                            row.snapshot_revision = snapshot.revision;
                            row.valid_until =
                                chrono::DateTime::from_timestamp(snapshot.valid_until as i64, 0)
                                    .map_or(String::new(), |time| time.to_rfc3339());
                            row.snapshot_age = self.now.0.saturating_sub(snapshot.issued_at);
                        }
                    } else {
                        row.effective.clear();
                        row.reason = "Unavailable".into();
                    }
                    if let Some(local_override) = local_override {
                        row.local_override = local_override.0.encode()
                    }
                    rows.push(row);
                    bound = true;
                }
                if !bound {
                    rows.push(InspectionRow::from_declaration(declaration))
                }
            }
        }
        rows.sort_by(|left, right| (&left.key, &left.scope).cmp(&(&right.key, &right.scope)));
        rows
    }

    fn target(
        &self,
        key: &str,
        environment: &str,
        scope: &str,
        generation: u64,
    ) -> Option<(Entity, &crate::flag::Declaration)> {
        if !self.allows_overrides() {
            return None;
        }
        let mut target = None;
        for (entity, definition, parent, _, _) in &self.evaluations {
            let Ok(context) = self.contexts.get(definition.scope) else {
                continue;
            };
            if context.subject.key != scope
                || context.generation != generation
                || !self
                    .providers
                    .get(parent.parent())
                    .is_ok_and(|name| name.as_str() == environment)
            {
                continue;
            }
            let Ok((_, source)) = self.sources.get(definition.source) else {
                continue;
            };
            let Some(declaration) = source.flags.get(definition.index) else {
                continue;
            };
            if declaration.key != key {
                continue;
            }
            if target.is_some() {
                return None;
            }
            target = Some((entity, declaration));
        }
        target
    }
}

impl InspectionRow {
    fn from_declaration(declaration: &crate::flag::Declaration) -> Self {
        Self {
            key: declaration.key.clone(),
            value_type: format!("{:?}", declaration.value_type),
            class: format!("{:?}", declaration.class),
            owner: declaration.owner.clone(),
            expiry: declaration.expiry.clone().unwrap_or_default(),
            lifecycle: format!("{:?}", declaration.lifecycle),
            default: declaration.default.encode(),
            effective: String::new(),
            source: "Default".into(),
            reason: "Unbound".into(),
            ..Default::default()
        }
    }
}

fn apply(
    trigger: On<UiInput<FeatureOverrideRequest>>,
    inspection: Inspection,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Some((target, declaration)) = inspection.target(
        &request.key,
        &request.environment,
        &request.scope,
        request.context_generation,
    ) else {
        commands
            .entity(trigger.event().webview)
            .insert(OverrideOutcome("features-override-unavailable".into()));
        return;
    };
    let value = match Value::decode(declaration.value_type, &request.value) {
        Ok(value) => value,
        Err(_) => {
            commands
                .entity(trigger.event().webview)
                .insert(OverrideOutcome("features-override-invalid".into()));
            return;
        }
    };
    commands.entity(target).insert(DevelopmentOverride(value));
    commands
        .entity(trigger.event().webview)
        .insert(OverrideOutcome(String::new()));
}

fn clear(
    trigger: On<UiInput<FeatureOverrideClearRequest>>,
    inspection: Inspection,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Some((target, _)) = inspection.target(
        &request.key,
        &request.environment,
        &request.scope,
        request.context_generation,
    ) else {
        commands
            .entity(trigger.event().webview)
            .insert(OverrideOutcome("features-override-unavailable".into()));
        return;
    };
    commands.entity(target).remove::<DevelopmentOverride>();
    commands
        .entity(trigger.event().webview)
        .insert(OverrideOutcome(String::new()));
}

fn retire(
    trigger: On<UiInput<FeatureAssignmentRetireRequest>>,
    inspection: Inspection,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    if !inspection.allows_overrides()
        || !inspection.sources.iter().any(|(_, manifest)| {
            manifest
                .flags
                .iter()
                .any(|declaration| declaration.key == request.key)
        })
    {
        commands
            .entity(trigger.event().webview)
            .insert(OverrideOutcome("features-assignment-unavailable".into()));
        return;
    }
    for (_, environment, binding) in &inspection.environments {
        if environment.as_str() != request.environment {
            continue;
        }
        commands.write_message(crate::cache::RetireAssignments {
            store: binding.store,
            environment: request.environment.clone(),
            flag: request.key.clone(),
            generation: request.generation.clone(),
        });
    }
}
