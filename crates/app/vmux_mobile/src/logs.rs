//! Where the phone writes down what happened.
//!
//! A simulator's stderr scrolls past inside the dev server and a real device has none at all, so
//! anything not written to a file is gone by the time someone asks what went wrong.

use std::io::Write;
use std::path::PathBuf;

use tracing_subscriber::EnvFilter;

/// The app's own log file, inside its sandbox container.
pub struct Logs;

impl Logs {
    /// Begin recording, and keep recording through a panic.
    ///
    /// Called before the event loop starts, so a failure during the first render is still caught.
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
        // The guard flushes on drop, and there is nowhere to hold one for the life of the process
        // that a panic would not skip past anyway.
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

    /// Append panics to the same file, synchronously.
    ///
    /// Not through the tracing writer: a panic crossing an `extern "C"` frame aborts the process
    /// immediately, and a background writer thread does not get to flush first. That is exactly
    /// the case that leaves nothing behind to read, so this one write bypasses the buffer.
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

    /// `$HOME` is the app's sandbox container on iOS, and an ordinary home directory elsewhere.
    pub fn directory() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/Application Support/Vmux Remote")
                .join("logs"),
        )
    }

    /// Panics go to a fixed name rather than the rolling file, so nothing has to agree with the
    /// appender about which date it decided on.
    fn panic_path() -> Option<PathBuf> {
        Some(Self::directory()?.join("vmux-mobile-panic.log"))
    }
}
