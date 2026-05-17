use std::sync::Arc;

use crossbeam_channel::Sender;
use futures_util::StreamExt;

use crate::app::AppAgentStrategy;
use crate::stream::StreamEvent;

pub async fn drive_sse(
    request: reqwest::Request,
    strategy: Arc<dyn AppAgentStrategy>,
    tx: Sender<StreamEvent>,
) {
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
            if let Some(event) = strategy.parse_sse_event(frame)
                && tx.send(event).is_err()
            {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::AgentStrategy;
    use crate::stream::ToolDef;
    use crate::{AgentKind, AgentVariant};
    use crossbeam_channel::unbounded;

    struct EchoTextStrategy;
    impl AgentStrategy for EchoTextStrategy {
        fn kind(&self) -> AgentKind {
            AgentKind::Vibe
        }
        fn variant(&self) -> AgentVariant {
            AgentVariant::App
        }
    }
    impl AppAgentStrategy for EchoTextStrategy {
        fn provider(&self) -> &str {
            "echo"
        }
        fn model(&self) -> &str {
            "echo"
        }
        fn endpoint(&self) -> &str {
            "stub://"
        }
        fn env_var(&self) -> &'static str {
            ""
        }
        fn build_request(
            &self,
            _: &str,
            _: &[crate::message::Message],
            _: &[ToolDef],
            _: &str,
        ) -> reqwest::Request {
            unreachable!("test builds request manually")
        }
        fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent> {
            payload
                .strip_prefix("data: ")
                .map(|s| StreamEvent::TextDelta(s.to_string()))
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn drives_two_text_deltas_from_mock_server() {
        let mut server = mockito::Server::new_async().await;
        let body = "data: hello\n\ndata: world\n\n";
        let _m = server
            .mock("POST", "/test")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(body)
            .create_async()
            .await;
        let req = reqwest::Client::new()
            .post(format!("{}/test", server.url()))
            .build()
            .unwrap();
        let (tx, rx) = unbounded::<StreamEvent>();
        drive_sse(req, Arc::new(EchoTextStrategy), tx).await;
        let collected: Vec<StreamEvent> = rx.try_iter().collect();
        assert_eq!(
            collected,
            vec![
                StreamEvent::TextDelta("hello".into()),
                StreamEvent::TextDelta("world".into())
            ]
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn http_error_status_emits_error_event() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/fail")
            .with_status(401)
            .with_body("unauthorized")
            .create_async()
            .await;
        let req = reqwest::Client::new()
            .post(format!("{}/fail", server.url()))
            .build()
            .unwrap();
        let (tx, rx) = unbounded::<StreamEvent>();
        drive_sse(req, Arc::new(EchoTextStrategy), tx).await;
        let collected: Vec<StreamEvent> = rx.try_iter().collect();
        assert_eq!(collected.len(), 1);
        match &collected[0] {
            StreamEvent::Error(msg) => {
                assert!(msg.contains("401"));
                assert!(msg.contains("unauthorized"));
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }
}
