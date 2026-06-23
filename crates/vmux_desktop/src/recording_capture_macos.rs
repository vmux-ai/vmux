use super::{CropRect, RecordOutcome, WakeFn};
use bevy::prelude::Entity;
use crossbeam_channel::Sender;
use vmux_agent::RecordStartResponse;

#[allow(clippy::too_many_arguments)]
pub(crate) fn start(
    _window_entity: Entity,
    _img_w: u32,
    _img_h: u32,
    _crop: Option<CropRect>,
    request_id: [u8; 16],
    _gif: bool,
    _max_secs: u32,
    _tx: Sender<RecordOutcome>,
    _wake: Option<WakeFn>,
) -> RecordStartResponse {
    RecordStartResponse {
        request_id,
        result: Err("recording not yet implemented".to_string()),
    }
}

pub(crate) fn stop(_request_id: [u8; 16], _dir: Option<String>, _name: Option<String>) {}

pub(crate) fn poll_auto_stop() {}
