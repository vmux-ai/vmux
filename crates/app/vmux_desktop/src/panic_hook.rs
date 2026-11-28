use std::panic::PanicHookInfo;
use vmux_profile::ServicePaths;

pub struct PanicHook;

impl PanicHook {
    pub fn install() {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let record = Self::crash_record_from(info);
            Self::write_crash(&record);
            previous(info);
        }));
    }

    fn crash_record_from(info: &PanicHookInfo<'_>) -> String {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown location>".to_string());
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let ts = chrono::Local::now().to_rfc3339();
        let thread = std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_string();
        Self::format_crash_record(&ts, &thread, &location, &message, &backtrace)
    }

    fn write_crash(record: &str) {
        crate::fs_io::DesktopFsIo::append_crash(
            &ServicePaths::log_dir(),
            &ServicePaths::current().current_log(),
            record,
        );
    }

    fn format_crash_record(
        ts: &str,
        thread: &str,
        location: &str,
        message: &str,
        backtrace: &str,
    ) -> String {
        format!("[{ts}] PANIC thread={thread} at {location}\n{message}\n{backtrace}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::PanicHook;

    #[test]
    fn format_crash_record_contains_message_and_location() {
        let out = PanicHook::format_crash_record(
            "2026-06-16T12:00:00Z",
            "main",
            "crates/vmux_desktop/src/main.rs:42:9",
            "boom",
            "<backtrace>",
        );
        assert!(out.contains("boom"), "got {out}");
        assert!(
            out.contains("crates/vmux_desktop/src/main.rs:42:9"),
            "got {out}"
        );
        assert!(out.contains("thread=main"), "got {out}");
        assert!(out.contains("PANIC"), "got {out}");
    }
}
