use bevy::app::App;
use bevy::log::BoxedLayer;
use tracing_subscriber::Layer;

pub fn file_log_layer(_app: &mut App) -> Option<BoxedLayer> {
    let dir = vmux_service::ServicePaths::log_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(format!(
            "vmux-{}",
            vmux_service::ServicePaths::build_profile()
        ))
        .filename_suffix("log")
        .max_log_files(7)
        .build(&dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    Box::leak(Box::new(guard));
    Some(
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_ansi(false)
            .boxed(),
    )
}
