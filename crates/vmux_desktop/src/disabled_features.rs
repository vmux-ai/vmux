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
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;

    #[cfg(not(feature = "screenshots"))]
    #[test]
    fn screenshot_requests_receive_disabled_response() {
        let mut app = App::new();
        app.add_message::<vmux_agent::ScreenshotRequest>()
            .add_message::<vmux_agent::ScreenshotResponse>()
            .add_systems(Update, reject_screenshots);
        app.world_mut()
            .resource_mut::<Messages<vmux_agent::ScreenshotRequest>>()
            .write(vmux_agent::ScreenshotRequest {
                request_id: [7; 16],
                pane: None,
            });

        app.update();

        let responses = app
            .world()
            .resource::<Messages<vmux_agent::ScreenshotResponse>>();
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
        app.add_message::<vmux_agent::RecordStartRequest>()
            .add_message::<vmux_agent::RecordStartResponse>()
            .add_message::<vmux_agent::RecordStopRequest>()
            .add_message::<vmux_agent::RecordStopResponse>()
            .add_systems(Update, (reject_recording_starts, reject_recording_stops));
        app.world_mut()
            .resource_mut::<Messages<vmux_agent::RecordStartRequest>>()
            .write(vmux_agent::RecordStartRequest {
                request_id: [8; 16],
                gif: false,
                max_secs: 30,
                pane: None,
            });
        app.world_mut()
            .resource_mut::<Messages<vmux_agent::RecordStopRequest>>()
            .write(vmux_agent::RecordStopRequest {
                request_id: [9; 16],
                dir: None,
                name: None,
            });

        app.update();

        let starts = app
            .world()
            .resource::<Messages<vmux_agent::RecordStartResponse>>();
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
        let stops = app
            .world()
            .resource::<Messages<vmux_agent::RecordStopResponse>>();
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
        app.init_resource::<vmux_setting::event::CurrentUpdateCheckStatus>()
            .add_message::<vmux_setting::event::CheckForUpdatesRequest>()
            .add_systems(Update, reject_update_checks);
        app.world_mut()
            .resource_mut::<Messages<vmux_setting::event::CheckForUpdatesRequest>>()
            .write(vmux_setting::event::CheckForUpdatesRequest);

        app.update();

        assert_eq!(
            app.world()
                .resource::<vmux_setting::event::CurrentUpdateCheckStatus>()
                .0,
            vmux_setting::event::UpdateCheckStatus::Unavailable
        );
    }
}
