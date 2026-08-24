use std::io::Write;
use std::path::PathBuf;

use tracing_subscriber::EnvFilter;

pub struct Logs;

impl Logs {
    pub fn start() {
        let Some(directory) = Self::directory() else {
            return;
        };
        if std::fs::create_dir_all(&directory).is_err() {
            return;
        }

        let appender = tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("vmux-mobile")
            .filename_suffix("log")
            .max_log_files(7)
            .build(&directory);
        let Ok(appender) = appender else {
            return;
        };

        let (writer, guard) = tracing_appender::non_blocking(appender);
        Box::leak(Box::new(guard));

        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_env("VMUX_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .with_writer(writer)
            .with_ansi(false)
            .try_init();

        Self::record_panics();
    }

    fn record_panics() {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Some(path) = Self::panic_path()
                && let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
            {
                let _ = writeln!(file, "panic: {info}");
                let _ = writeln!(
                    file,
                    "backtrace:\n{}",
                    std::backtrace::Backtrace::force_capture()
                );
                let _ = file.flush();
            }
            previous(info);
        }));
    }

    pub fn directory() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/Application Support/Vmux Remote")
                .join("logs"),
        )
    }

    fn panic_path() -> Option<PathBuf> {
        Some(Self::directory()?.join("vmux-mobile-panic.log"))
    }
}
