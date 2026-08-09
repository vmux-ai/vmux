use crossbeam_channel::Sender;
use futures_util::StreamExt;

use crate::stream::{ParseSse, StreamEvent};

pub async fn drive_sse(request: reqwest::Request, parse_sse: ParseSse, tx: Sender<StreamEvent>) {
    let client = reqwest::Client::new();
    let response = match client.execute(request).await {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(StreamEvent::Error(format!("HTTP request failed: {e}")));
            return;
        }
    };
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(500).collect();
        let _ = tx.send(StreamEvent::Error(format!("HTTP {status}: {snippet}")));
        return;
    }
    let mut stream = response.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        let bytes = match chunk {
            Ok(b) => b,
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(format!("stream chunk: {e}")));
                return;
            }
        };
        buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(idx) = buf.find("\n\n") {
            let frame: String = buf.drain(..idx + 2).collect();
            let frame = frame.trim_end_matches('\n');
            if frame.is_empty() {
                continue;
            }
            if let Some(event) = parse_sse(frame)
                && tx.send(event).is_err()
            {
                return;
            }
        }
    }
}

#[cfg(test)]
#[path = "http.test.rs"]
mod tests;
