use std::fs::OpenOptions;
use std::io::Write;
use std::panic::PanicHookInfo;

pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let record = crash_record_from(info);
        write_crash(&record);
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
    format_crash_record(&ts, &thread, &location, &message, &backtrace)
}

fn write_crash(record: &str) {
    let _ = std::fs::create_dir_all(vmux_service::log_dir());
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(vmux_service::current_log_file())
    {
        let _ = file.write_all(record.as_bytes());
    }
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

#[cfg(test)]
#[path = "panic_hook.test.rs"]
mod tests;
