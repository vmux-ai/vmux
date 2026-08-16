use crate::common::custom_scheme::asset_loader::CefResponseHandle;
use crate::common::{ResolvedWebviewUri, WebviewSource};
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy_cef_core::prelude::*;

pub struct ResponserPlugin;

impl Plugin for ResponserPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = async_channel::unbounded();
        app.insert_resource(Requester(tx))
            .insert_resource(RequesterReceiver(rx))
            .add_systems(PreUpdate, resolve_webview_source)
            .add_systems(
                Update,
                (
                    coming_request,
                    responser,
                    hot_reload.run_if(any_changed_assets),
                ),
            );
    }
}

fn any_changed_assets(mut er: MessageReader<AssetEvent<CefResponse>>) -> bool {
    er.read()
        .any(|event| matches!(event, AssetEvent::Modified { .. }))
}

fn resolve_webview_source(
    mut commands: Commands,
    query: Query<
        (Entity, &WebviewSource, Option<&ResolvedWebviewUri>),
        Or<(Added<WebviewSource>, Changed<WebviewSource>)>,
    >,
) {
    for (entity, source, existing_resolved) in query.iter() {
        if existing_resolved.is_some_and(|resolved| resolved.0 == source.0) {
            continue;
        }
        commands
            .entity(entity)
            .insert(ResolvedWebviewUri(source.0.clone()));
    }
}

fn coming_request(
    mut commands: Commands,
    requester_receiver: Res<RequesterReceiver>,
    asset_server: Res<AssetServer>,
) {
    while let Ok(request) = requester_receiver.0.try_recv() {
        commands.spawn((
            CefResponseHandle(asset_server.load(request.uri)),
            request.responser,
        ));
    }
}

fn responser(
    mut commands: Commands,
    mut handle_stores: Local<HashSet<Handle<CefResponse>>>,
    responses: Res<Assets<CefResponse>>,
    asset_server: Res<AssetServer>,
    handles: Query<(Entity, &CefResponseHandle, &Responser)>,
) {
    for (entity, handle, responser) in handles.iter() {
        if let Some(response) = responses.get(&handle.0) {
            let _ = responser.0.send_blocking(response.clone());
            commands.entity(entity).despawn();
            handle_stores.insert(handle.0.clone());
        } else if matches!(
            asset_server.load_state(&handle.0),
            bevy::asset::LoadState::Failed(_)
        ) {
            error!("custom scheme asset load failed: {:?}", handle.0.path());
            let _ = responser.0.send_blocking(CefResponse {
                mime_type: "text/plain".to_string(),
                status_code: 404,
                data: b"Asset load failed".to_vec(),
            });
            commands.entity(entity).despawn();
        }
    }
}

fn hot_reload(browsers: NonSend<Browsers>) {
    browsers.reload();
}
