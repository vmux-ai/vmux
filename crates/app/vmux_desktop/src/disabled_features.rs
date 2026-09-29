use bevy::prelude::*;
#[cfg(not(feature = "recording"))]
use vmux_capture::{
    RecordStartRequest, RecordStartResponse, RecordStopRequest, RecordStopResponse,
};
#[cfg(not(feature = "screenshots"))]
use vmux_capture::{ScreenshotRequest, ScreenshotResponse};
#[cfg(any(not(feature = "screenshots"), not(feature = "recording")))]
use vmux_command::WriteCommandRequests;
#[cfg(not(feature = "updater"))]
use vmux_setting::event::{CheckForUpdatesRequest, CurrentUpdateCheckStatus, UpdateCheckStatus};

#[cfg(not(feature = "screenshots"))]
pub(crate) struct ScreenshotsDisabledPlugin;

#[cfg(not(feature = "screenshots"))]
impl Plugin for ScreenshotsDisabledPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, reject_screenshots.after(WriteCommandRequests));
    }
}

#[cfg(not(feature = "recording"))]
pub(crate) struct RecordingDisabledPlugin;

#[cfg(not(feature = "recording"))]
impl Plugin for RecordingDisabledPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (reject_recording_starts, reject_recording_stops).after(WriteCommandRequests),
        );
    }
}

#[cfg(not(feature = "updater"))]
pub(crate) struct UpdaterDisabledPlugin;

#[cfg(not(feature = "updater"))]
impl Plugin for UpdaterDisabledPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, mark_updater_unavailable)
            .add_systems(Update, reject_update_checks);
    }
}

#[cfg(not(feature = "screenshots"))]
fn reject_screenshots(
    mut requests: MessageReader<ScreenshotRequest>,
    mut responses: MessageWriter<ScreenshotResponse>,
) {
    for request in requests.read() {
        responses.write(ScreenshotResponse {
            request_id: request.request_id,
            result: Err("screenshots are disabled in this build".to_string()),
        });
    }
}

#[cfg(not(feature = "recording"))]
fn reject_recording_starts(
    mut requests: MessageReader<RecordStartRequest>,
    mut responses: MessageWriter<RecordStartResponse>,
) {
    for request in requests.read() {
        responses.write(RecordStartResponse {
            request_id: request.request_id,
            result: Err("recording is disabled in this build".to_string()),
        });
    }
}

#[cfg(not(feature = "recording"))]
fn reject_recording_stops(
    mut requests: MessageReader<RecordStopRequest>,
    mut responses: MessageWriter<RecordStopResponse>,
) {
    for request in requests.read() {
        responses.write(RecordStopResponse {
            request_id: request.request_id,
            result: Err("recording is disabled in this build".to_string()),
        });
    }
}

#[cfg(not(feature = "updater"))]
fn mark_updater_unavailable(mut status: Single<&mut CurrentUpdateCheckStatus>) {
    status.0 = UpdateCheckStatus::Unavailable;
}

#[cfg(not(feature = "updater"))]
fn reject_update_checks(
    mut requests: MessageReader<CheckForUpdatesRequest>,
    mut status: Single<&mut CurrentUpdateCheckStatus>,
) {
    if requests.read().count() > 0 {
        status.0 = UpdateCheckStatus::Unavailable;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;

    #[cfg(not(feature = "screenshots"))]
    #[test]
    fn screenshot_requests_receive_disabled_response() {
        let mut app = App::new();
        app.add_message::<ScreenshotRequest>()
            .add_message::<ScreenshotResponse>()
            .add_systems(Update, reject_screenshots);
        app.world_mut()
            .resource_mut::<Messages<ScreenshotRequest>>()
            .write(ScreenshotRequest {
                request_id: [7; 16],
                pane: None,
            });

        app.update();

        let responses = app.world().resource::<Messages<ScreenshotResponse>>();
        let mut cursor = responses.get_cursor();
        let response = cursor.read(responses).next().expect("disabled response");
        assert_eq!(response.request_id, [7; 16]);
        assert!(matches!(
            &response.result,
            Err(message) if message == "screenshots are disabled in this build"
        ));
    }

    #[cfg(not(feature = "recording"))]
    #[test]
    fn recording_requests_receive_disabled_responses() {
        let mut app = App::new();
        app.add_message::<RecordStartRequest>()
            .add_message::<RecordStartResponse>()
            .add_message::<RecordStopRequest>()
            .add_message::<RecordStopResponse>()
            .add_systems(Update, (reject_recording_starts, reject_recording_stops));
        app.world_mut()
            .resource_mut::<Messages<RecordStartRequest>>()
            .write(RecordStartRequest {
                request_id: [8; 16],
                gif: false,
                max_secs: 30,
                pane: None,
            });
        app.world_mut()
            .resource_mut::<Messages<RecordStopRequest>>()
            .write(RecordStopRequest {
                request_id: [9; 16],
                dir: None,
                name: None,
            });

        app.update();

        let starts = app.world().resource::<Messages<RecordStartResponse>>();
        let mut start_cursor = starts.get_cursor();
        assert_eq!(
            start_cursor
                .read(starts)
                .next()
                .expect("disabled start response")
                .result
                .as_ref()
                .unwrap_err(),
            "recording is disabled in this build"
        );
        let stops = app.world().resource::<Messages<RecordStopResponse>>();
        let mut stop_cursor = stops.get_cursor();
        let response = stop_cursor
            .read(stops)
            .next()
            .expect("disabled stop response");
        assert!(matches!(
            &response.result,
            Err(message) if message == "recording is disabled in this build"
        ));
    }

    #[cfg(not(feature = "updater"))]
    #[test]
    fn update_requests_fail_in_disabled_builds() {
        let mut app = App::new();
        app.add_message::<CheckForUpdatesRequest>()
            .add_systems(Update, reject_update_checks);
        app.world_mut().spawn(CurrentUpdateCheckStatus::default());
        app.world_mut()
            .resource_mut::<Messages<CheckForUpdatesRequest>>()
            .write(CheckForUpdatesRequest);

        app.update();

        let mut statuses = app.world_mut().query::<&CurrentUpdateCheckStatus>();
        assert_eq!(
            statuses.single(app.world()).unwrap().0,
            UpdateCheckStatus::Unavailable
        );
    }
}
