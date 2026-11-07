use crate::common::custom_scheme::asset_loader::CefResponseHandle;
use crate::common::{ResolvedWebviewUri, WebviewSource};
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
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
                    (coming_request, responser).chain(),
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
    proxy: Option<Res<EventLoopProxyWrapper>>,
    wake_policy: Option<Res<MessageLoopWakePolicy>>,
) {
    let mut pending = false;
    for (entity, handle, responser) in &handles {
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
        } else {
            pending = true;
        }
    }
    if pending {
        if let Some(wake_policy) = wake_policy {
            wake_policy.begin_interaction();
        }
        if let Some(proxy) = proxy {
            let _ = proxy.send_event(WinitUserEvent::WakeUp);
        }
    }
}

fn hot_reload(browsers: NonSend<Browsers>) {
    browsers.reload();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::custom_scheme::asset_loader::LocalSchemeAssetLoaderPlugin;
    use bevy::asset::io::AssetSourceBuilder;
    use bevy::asset::io::memory::MemoryAssetReader;
    use std::path::Path;
    use std::time::{Duration, Instant};

    #[test]
    fn asset_requests_wake_the_loop_in_the_receiving_update_and_settle_after_delivery() {
        let reader = MemoryAssetReader::default();
        let css = "body { background: #123456; }";
        reader.root.insert_asset_text(Path::new("index.css"), css);
        let policy = MessageLoopWakePolicy::default();
        policy.set_min_wake_interval(Duration::from_secs(1));
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .register_asset_source(
                "test",
                AssetSourceBuilder::new(move || Box::new(reader.clone())),
            )
            .add_plugins((
                AssetPlugin::default(),
                LocalSchemeAssetLoaderPlugin,
                ResponserPlugin,
            ))
            .insert_resource(policy.clone());
        let (tx, rx) = async_channel::bounded(1);
        app.world()
            .resource::<Requester>()
            .try_send(CefRequest {
                uri: "test://index.css".to_string(),
                responser: Responser(tx),
            })
            .unwrap();

        app.update();

        assert_eq!(policy.min_wake_interval(), Duration::from_millis(16));
        let deadline = Instant::now() + Duration::from_secs(2);
        let response = loop {
            if let Ok(response) = rx.try_recv() {
                break response;
            }
            assert!(Instant::now() < deadline, "CSS response was not delivered");
            std::thread::yield_now();
            app.update();
        };
        assert_eq!(response.status_code, 200);
        assert_eq!(response.mime_type, "text/css");
        assert_eq!(response.data, css.as_bytes());

        std::thread::sleep(Duration::from_millis(300));
        app.update();

        assert_eq!(policy.min_wake_interval(), Duration::from_secs(1));
    }
}
