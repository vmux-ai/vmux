use bevy::prelude::*;

pub(super) struct FallbackBrowserPlugin;

impl Plugin for FallbackBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MissingRendererReported>()
            .add_systems(Update, report_missing_renderer);
    }
}

#[derive(Resource, Default)]
struct MissingRendererReported(bool);

fn report_missing_renderer(mut reported: ResMut<MissingRendererReported>) {
    if !reported.0 {
        reported.0 = true;
        warn!("browser_platform: no renderer on this platform, hosted pages will be missing");
    }
}
