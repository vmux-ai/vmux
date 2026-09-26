use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::AgentQuery;
use vmux_tool::{AddedTool, ToolAppExt, ToolDispatchSet, ToolManifestPlugin, ToolQuery};

pub struct CaptureToolPlugin;

impl Plugin for CaptureToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::new(include_str!("capture_tool.ron")))
            .register_tool::<ScreenshotArgs>("screenshot")
            .register_tool::<RecordStartArgs>("record_start")
            .register_tool::<RecordStopArgs>("record_stop")
            .add_systems(
                Update,
                (screenshot, record_start, record_stop).in_set(ToolDispatchSet),
            );
    }
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenshotArgs {
    pane: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordStartArgs {
    #[serde(default)]
    gif: bool,
    max_secs: Option<u32>,
    pane: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordStopArgs {
    dir: Option<String>,
    name: Option<String>,
}

fn screenshot(
    mut commands: Commands,
    requests: Query<(Entity, &ScreenshotArgs), AddedTool<ScreenshotArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::Screenshot {
                pane: OptionalText::trim(args.pane.clone()),
            })));
    }
}

fn record_start(
    mut commands: Commands,
    requests: Query<(Entity, &RecordStartArgs), AddedTool<RecordStartArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::RecordStart {
                gif: args.gif,
                max_secs: args.max_secs.unwrap_or(600),
                pane: OptionalText::trim(args.pane.clone()),
            })));
    }
}

fn record_stop(
    mut commands: Commands,
    requests: Query<(Entity, &RecordStopArgs), AddedTool<RecordStopArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::RecordStop {
                dir: OptionalText::trim(args.dir.clone()),
                name: OptionalText::trim(args.name.clone()),
            })));
    }
}

struct OptionalText;

impl OptionalText {
    fn trim(value: Option<String>) -> Option<String> {
        value.and_then(|value| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::JsonArguments;
    use vmux_tool::{ToolCatalog, ToolCatalogRequest, ToolDispatchError, ToolInvocation};

    struct CaptureToolFixture;

    impl CaptureToolFixture {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(CaptureToolPlugin);
            app.update();
            app
        }

        fn definitions() -> Vec<String> {
            let mut app = Self::app();
            let request = app.world_mut().spawn(ToolCatalogRequest).id();
            app.update();
            app.world_mut()
                .entity_mut(request)
                .take::<ToolCatalog>()
                .unwrap()
                .0
                .into_iter()
                .map(|definition| definition.name)
                .collect()
        }

        fn dispatch(name: &str, arguments: serde_json::Value) -> Result<AgentQuery, String> {
            let mut app = Self::app();
            let request = app
                .world_mut()
                .spawn((
                    Name::new(name.to_string()),
                    JsonArguments(arguments),
                    ToolInvocation,
                ))
                .id();
            app.update();
            if let Some(query) = app.world_mut().entity_mut(request).take::<ToolQuery>() {
                return query.0;
            }
            let error = app
                .world_mut()
                .entity_mut(request)
                .take::<ToolDispatchError>()
                .unwrap();
            Err(error.message().to_string())
        }
    }

    #[test]
    fn manifest_registers_capture_tools() {
        assert_eq!(
            CaptureToolFixture::definitions(),
            ["screenshot", "record_start", "record_stop"]
        );
    }

    #[test]
    fn screenshot_dispatches_optional_pane() {
        assert_eq!(
            CaptureToolFixture::dispatch("screenshot", serde_json::json!({})),
            Ok(AgentQuery::Screenshot { pane: None })
        );
        assert_eq!(
            CaptureToolFixture::dispatch("screenshot", serde_json::json!({"pane": "pane:7"})),
            Ok(AgentQuery::Screenshot {
                pane: Some("pane:7".to_string()),
            })
        );
    }

    #[test]
    fn recording_dispatches_defaults_and_output() {
        assert_eq!(
            CaptureToolFixture::dispatch("record_start", serde_json::json!({})),
            Ok(AgentQuery::RecordStart {
                gif: false,
                max_secs: 600,
                pane: None,
            })
        );
        assert_eq!(
            CaptureToolFixture::dispatch(
                "record_start",
                serde_json::json!({"gif": true, "max_secs": 30, "pane": "pane:3"}),
            ),
            Ok(AgentQuery::RecordStart {
                gif: true,
                max_secs: 30,
                pane: Some("pane:3".to_string()),
            })
        );
        assert_eq!(
            CaptureToolFixture::dispatch(
                "record_stop",
                serde_json::json!({"dir": "/tmp/out", "name": "feature-x"}),
            ),
            Ok(AgentQuery::RecordStop {
                dir: Some("/tmp/out".to_string()),
                name: Some("feature-x".to_string()),
            })
        );
    }
}
