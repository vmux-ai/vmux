use std::collections::VecDeque;
use std::fmt::Write as _;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::app::App;
use bevy::log::BoxedLayer;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

use vmux_service::protocol::{ClientMessage, level_to_u8};

#[derive(Clone, Debug, PartialEq)]
struct LogRecord {
    ts_ms: u64,
    level: u8,
    target: String,
    message: String,
}

const CHANNEL_CAP: usize = 1024;
const BUFFER_CAP: usize = 1024;

fn forward_threshold() -> tracing::Level {
    match std::env::var("VMUX_LOG_FORWARD") {
        Ok(v) => match v.trim().to_ascii_lowercase().as_str() {
            "error" => tracing::Level::ERROR,
            "warn" => tracing::Level::WARN,
            "info" => tracing::Level::INFO,
            "debug" => tracing::Level::DEBUG,
            "trace" => tracing::Level::TRACE,
            _ => tracing::Level::INFO,
        },
        Err(_) => tracing::Level::INFO,
    }
}

fn enqueue_drop_oldest(buf: &mut VecDeque<LogRecord>, cap: usize, rec: LogRecord) {
    if buf.len() >= cap {
        buf.pop_front();
    }
    buf.push_back(rec);
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
    extra: String,
}

impl MessageVisitor {
    fn into_message(self) -> String {
        if self.extra.is_empty() {
            self.message
        } else {
            format!("{}{}", self.message, self.extra)
        }
    }
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.extra, " {}={:?}", field.name(), value);
        }
    }
}

struct IpcForwardLayer {
    tx: SyncSender<LogRecord>,
    threshold: tracing::Level,
}

impl<S: Subscriber> Layer<S> for IpcForwardLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        if *meta.level() > self.threshold {
            return;
        }
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let rec = LogRecord {
            ts_ms: now_ms(),
            level: level_to_u8(*meta.level()),
            target: meta.target().to_string(),
            message: visitor.into_message(),
        };
        let _ = self.tx.try_send(rec);
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn send_record(stream: &mut UnixStream, rec: &LogRecord) -> std::io::Result<()> {
    vmux_service::client::send_message_blocking(
        stream,
        &ClientMessage::Log {
            ts_ms: rec.ts_ms,
            level: rec.level,
            target: rec.target.clone(),
            message: rec.message.clone(),
        },
    )
}

fn spawn_forward_thread(rx: Receiver<LogRecord>) {
    std::thread::Builder::new()
        .name("vmux-log-forward".into())
        .spawn(move || {
            let mut buf: VecDeque<LogRecord> = VecDeque::with_capacity(BUFFER_CAP);
            let mut stream: Option<UnixStream> = None;
            loop {
                let Ok(first) = rx.recv() else {
                    return;
                };
                enqueue_drop_oldest(&mut buf, BUFFER_CAP, first);
                while let Ok(more) = rx.try_recv() {
                    enqueue_drop_oldest(&mut buf, BUFFER_CAP, more);
                }
                if stream.is_none() {
                    stream = UnixStream::connect(vmux_service::socket_path()).ok();
                    if let Some(s) = &stream {
                        let _ = s.set_write_timeout(Some(std::time::Duration::from_millis(500)));
                    }
                }
                if let Some(s) = stream.as_mut() {
                    while let Some(rec) = buf.front().cloned() {
                        match send_record(s, &rec) {
                            Ok(()) => {
                                buf.pop_front();
                            }
                            Err(_) => {
                                stream = None;
                                break;
                            }
                        }
                    }
                }
            }
        })
        .expect("spawn vmux-log-forward thread");
}

pub fn ipc_log_layer(_app: &mut App) -> Option<BoxedLayer> {
    let threshold = forward_threshold();
    let (tx, rx) = std::sync::mpsc::sync_channel::<LogRecord>(CHANNEL_CAP);
    spawn_forward_thread(rx);
    Some(Box::new(IpcForwardLayer { tx, threshold }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(n: u64) -> LogRecord {
        LogRecord {
            ts_ms: n,
            level: 3,
            target: "t".into(),
            message: format!("m{n}"),
        }
    }

    #[test]
    fn enqueue_drops_oldest_at_capacity() {
        let mut buf = VecDeque::new();
        enqueue_drop_oldest(&mut buf, 2, rec(1));
        enqueue_drop_oldest(&mut buf, 2, rec(2));
        enqueue_drop_oldest(&mut buf, 2, rec(3));
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.front().unwrap().ts_ms, 2);
        assert_eq!(buf.back().unwrap().ts_ms, 3);
    }

    #[test]
    fn forward_threshold_defaults_to_info() {
        unsafe { std::env::remove_var("VMUX_LOG_FORWARD") };
        assert_eq!(forward_threshold(), tracing::Level::INFO);
    }

    #[test]
    fn visitor_joins_message_and_extra_fields() {
        let v = MessageVisitor {
            message: "hello".into(),
            extra: " k=1".into(),
        };
        assert_eq!(v.into_message(), "hello k=1");
    }

    #[test]
    fn visitor_message_only_when_no_extra() {
        let v = MessageVisitor {
            message: "just msg".into(),
            extra: String::new(),
        };
        assert_eq!(v.into_message(), "just msg");
    }
}
