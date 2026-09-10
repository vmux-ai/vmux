use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive};
use vmux_core::event::{PAGE_CONTEXT_EVENT, PageContextEvent, PageContextRequest};

use crate::settings::EffectiveStartupDir;
use crate::tab::Tab;

pub struct PageContextPlugin;

impl Plugin for PageContextPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(PageContextRequest,)>::default())
            .add_observer(on_page_context_request);
    }
}

fn on_page_context_request(
    trigger: On<BinReceive<PageContextRequest>>,
    child_of: Query<&ChildOf>,
    tabs: Query<&Tab>,
    effective_dir: Option<Res<EffectiveStartupDir>>,
    mut commands: Commands,
) {
    let path = crate::tab::ancestor_tab_startup_dir(trigger.event().webview, &child_of, &tabs)
        .map(PathBuf::from)
        .or_else(|| {
            effective_dir
                .as_ref()
                .and_then(|effective| effective.0.as_ref())
                .and_then(|(_, path)| path.clone())
        })
        .or_else(|| std::env::current_dir().ok())
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    commands.trigger(BinHostEmitEvent::from_rkyv(
        trigger.event().webview,
        PAGE_CONTEXT_EVENT,
        &PageContextEvent {
            working_directory: path,
        },
    ));
}
