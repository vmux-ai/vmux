use bevy::prelude::*;

#[cfg(not(feature = "screenshots"))]
pub(crate) fn reject_screenshots(
    mut requests: MessageReader<vmux_agent::ScreenshotRequest>,
    mut responses: MessageWriter<vmux_agent::ScreenshotResponse>,
) {
    for request in requests.read() {
        responses.write(vmux_agent::ScreenshotResponse {
            request_id: request.request_id,
            result: Err("screenshots are disabled in this build".to_string()),
        });
    }
}

#[cfg(not(feature = "recording"))]
pub(crate) fn reject_recording_starts(
    mut requests: MessageReader<vmux_agent::RecordStartRequest>,
    mut responses: MessageWriter<vmux_agent::RecordStartResponse>,
) {
    for request in requests.read() {
        responses.write(vmux_agent::RecordStartResponse {
            request_id: request.request_id,
            result: Err("recording is disabled in this build".to_string()),
        });
    }
}

#[cfg(not(feature = "recording"))]
pub(crate) fn reject_recording_stops(
    mut requests: MessageReader<vmux_agent::RecordStopRequest>,
    mut responses: MessageWriter<vmux_agent::RecordStopResponse>,
) {
    for request in requests.read() {
        responses.write(vmux_agent::RecordStopResponse {
            request_id: request.request_id,
            result: Err("recording is disabled in this build".to_string()),
        });
    }
}

#[cfg(not(feature = "updater"))]
pub(crate) fn mark_updater_unavailable(
    mut status: ResMut<vmux_setting::event::CurrentUpdateCheckStatus>,
) {
    status.0 = vmux_setting::event::UpdateCheckStatus::Unavailable;
}

#[cfg(not(feature = "updater"))]
pub(crate) fn reject_update_checks(
    mut requests: MessageReader<vmux_setting::event::CheckForUpdatesRequest>,
    mut status: ResMut<vmux_setting::event::CurrentUpdateCheckStatus>,
) {
    if requests.read().count() > 0 {
        status.0 = vmux_setting::event::UpdateCheckStatus::Unavailable;
    }
}

#[cfg(test)]
#[path = "disabled_features.test.rs"]
mod tests;
