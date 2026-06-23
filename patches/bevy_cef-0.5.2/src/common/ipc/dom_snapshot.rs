use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::SnapshotResultRaw;

#[derive(Resource)]
pub struct SnapshotResultSender(pub Sender<SnapshotResultRaw>);

#[derive(Resource)]
pub struct SnapshotResultReceiver(pub Receiver<SnapshotResultRaw>);

#[derive(Message, Clone)]
pub struct SnapshotResult {
    pub webview: Entity,
    pub request_id: String,
    pub json: String,
}

fn drain_snapshot_results(
    receiver: Res<SnapshotResultReceiver>,
    mut writer: MessageWriter<SnapshotResult>,
) {
    while let Ok(raw) = receiver.0.try_recv() {
        writer.write(SnapshotResult {
            webview: raw.webview,
            request_id: raw.request_id,
            json: raw.json,
        });
    }
}

pub(crate) struct DomSnapshotPlugin;

impl Plugin for DomSnapshotPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = async_channel::unbounded();
        app.insert_resource(SnapshotResultSender(tx))
            .insert_resource(SnapshotResultReceiver(rx))
            .add_message::<SnapshotResult>()
            .add_systems(Update, drain_snapshot_results);
    }
}
