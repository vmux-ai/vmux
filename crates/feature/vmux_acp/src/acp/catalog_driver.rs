use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;
use std::task::{Context, Poll};
use std::time::Duration;

use agent_client_protocol::Client;
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{InitializeRequest, ListSessionsRequest};
use tokio::io::{AsyncRead, ReadBuf};
use tokio::process::Child;
use tokio::process::Command;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use vmux_api::protocol::AcpProviderSession;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_PAGES: usize = 100;
const MAX_SESSIONS: usize = 50_000;
const MAX_CATALOG_BYTES: usize = 32 * 1024 * 1024;
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

pub(crate) struct CatalogRequest {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
}

pub(crate) struct CatalogResult {
    pub(crate) supported: bool,
    pub(crate) sessions: Vec<AcpProviderSession>,
}

pub(crate) struct CatalogDriver;

struct CatalogProcess(Child);

struct BoundedLineReader<R> {
    inner: R,
    line_bytes: usize,
    max_line_bytes: usize,
}

#[derive(Default)]
struct CatalogBudget(usize);

impl Drop for CatalogProcess {
    fn drop(&mut self) {
        let _ = self.0.start_kill();
    }
}

impl CatalogBudget {
    fn accept(&mut self, session: &AcpProviderSession) -> bool {
        let bytes = session
            .id
            .len()
            .saturating_add(session.cwd.len())
            .saturating_add(session.title.len())
            .saturating_add(session.updated_at.len())
            .saturating_add(64);
        let next = self.0.saturating_add(bytes);
        if next > MAX_CATALOG_BYTES {
            return false;
        }
        self.0 = next;
        true
    }
}

impl<R> BoundedLineReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            line_bytes: 0,
            max_line_bytes: MAX_FRAME_BYTES,
        }
    }

    #[cfg(test)]
    fn with_limit(inner: R, max_line_bytes: usize) -> Self {
        Self {
            inner,
            line_bytes: 0,
            max_line_bytes,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for BoundedLineReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let mut bytes = vec![0; buffer.remaining()];
        let mut incoming = ReadBuf::new(&mut bytes);
        match Pin::new(&mut self.inner).poll_read(cx, &mut incoming) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                let mut line_bytes = self.line_bytes;
                for byte in incoming.filled() {
                    if *byte == b'\n' {
                        line_bytes = 0;
                    } else {
                        line_bytes = line_bytes.saturating_add(1);
                        if line_bytes > self.max_line_bytes {
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "ACP catalog frame exceeds the size limit",
                            )));
                        }
                    }
                }
                self.line_bytes = line_bytes;
                buffer.put_slice(incoming.filled());
                Poll::Ready(Ok(()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn bounded_reader_rejects_an_oversized_frame() {
        let (mut writer, reader) = tokio::io::duplex(32);
        writer.write_all(b"12345\n").await.unwrap();
        drop(writer);
        let mut reader = BoundedLineReader::with_limit(reader, 4);
        let mut output = Vec::new();

        let error = reader.read_to_end(&mut output).await.unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn catalog_budget_rejects_entries_beyond_the_frame_budget() {
        let mut budget = CatalogBudget(MAX_CATALOG_BYTES - 64);
        let session = AcpProviderSession {
            id: "x".into(),
            cwd: String::new(),
            title: String::new(),
            updated_at: String::new(),
        };

        assert!(!budget.accept(&session));
    }
}

impl CatalogDriver {
    pub(crate) async fn list(request: CatalogRequest) -> Result<CatalogResult, String> {
        if !request.cwd.is_dir() {
            return Err(format!(
                "ACP catalog cwd is unavailable: {}",
                request.cwd.display()
            ));
        }
        let child = Command::new(&request.command)
            .args(&request.args)
            .env_clear()
            .envs(request.env)
            .current_dir(request.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format!("ACP catalog spawn failed: {error}"))?;
        let mut process = CatalogProcess(child);
        let stdin = process
            .0
            .stdin
            .take()
            .ok_or("ACP catalog stdin is unavailable")?
            .compat_write();
        let stdout = BoundedLineReader::new(
            process
                .0
                .stdout
                .take()
                .ok_or("ACP catalog stdout is unavailable")?,
        )
        .compat();
        let transport = agent_client_protocol::ByteStreams::new(stdin, stdout);
        let result = tokio::time::timeout(REQUEST_TIMEOUT, async move {
            Client
                .connect_with(transport, async move |connection| {
                    let initialize = InitializeRequest::new(ProtocolVersion::V1);
                    let initialized = connection.send_request(initialize).block_task().await?;
                    if initialized
                        .agent_capabilities
                        .session_capabilities
                        .list
                        .is_none()
                    {
                        return Ok(CatalogResult {
                            supported: false,
                            sessions: Vec::new(),
                        });
                    }
                    let mut cursor = None;
                    let mut sessions = Vec::new();
                    let mut budget = CatalogBudget::default();
                    for _ in 0..MAX_PAGES {
                        let request = ListSessionsRequest::new().cursor(cursor.clone());
                        let response = connection.send_request(request).block_task().await?;
                        for session in response.sessions {
                            let session = AcpProviderSession {
                                id: session.session_id.to_string(),
                                cwd: session.cwd.to_string_lossy().into_owned(),
                                title: session.title.unwrap_or_default(),
                                updated_at: session.updated_at.unwrap_or_default(),
                            };
                            if sessions.len() == MAX_SESSIONS || !budget.accept(&session) {
                                return Ok(CatalogResult {
                                    supported: true,
                                    sessions,
                                });
                            }
                            sessions.push(session);
                        }
                        let next = response.next_cursor.filter(|next| !next.is_empty());
                        if next.is_none() || next == cursor {
                            break;
                        }
                        cursor = next;
                    }
                    Ok(CatalogResult {
                        supported: true,
                        sessions,
                    })
                })
                .await
                .map_err(|error| error.to_string())
        })
        .await
        .map_err(|_| "ACP session/list timed out".to_string())?;
        let _ = process.0.start_kill();
        let _ = tokio::time::timeout(SHUTDOWN_TIMEOUT, process.0.wait()).await;
        result
    }
}
