use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::path::Path;

use vmux_api::browser::{AgentOpenBrowser, Url};
use vmux_api::protocol::{
    AgentCommandResult, AgentRequest, AgentRequestId, ClientMessage, ServiceMessage,
};
use vmux_ecs::cli::CliInvocation;
use vmux_transport::service::ServiceCodec;

pub(crate) struct Open {
    url: Url,
    source: Option<String>,
}

impl TryFrom<&CliInvocation> for Open {
    type Error = String;

    fn try_from(invocation: &CliInvocation) -> Result<Self, Self::Error> {
        let url = invocation.value("url").ok_or("browser URL is missing")?;
        Ok(Self {
            url: Url::parse(url)?,
            source: invocation.value("source").map(str::to_string),
        })
    }
}

impl Open {
    pub(crate) fn send(&self, socket: &Path) -> Result<u8, String> {
        let mut stream = UnixStream::connect(socket)
            .map_err(|error| format!("could not connect to Vmux; open the app first: {error}"))?;
        stream
            .set_read_timeout(Some(vmux_api::protocol::AGENT_REQUEST_TIMEOUT))
            .map_err(|error| error.to_string())?;
        stream
            .set_write_timeout(Some(vmux_api::protocol::AGENT_REQUEST_TIMEOUT))
            .map_err(|error| error.to_string())?;
        let request_id = AgentRequestId::new();
        let request = AgentRequest::encode(&AgentOpenBrowser {
            url: self.url.as_str().to_string(),
            source: self.source.clone(),
        })?;
        ServiceCodec::write_client_blocking(
            &mut stream,
            &ClientMessage::AgentRequest {
                request_id,
                anchor: None,
                request,
            },
        )
        .map_err(|error| error.to_string())?;
        let mut reader = BufReader::new(&mut stream);
        while let Some(message) =
            ServiceCodec::read_service_blocking(&mut reader).map_err(|error| error.to_string())?
        {
            let ServiceMessage::AgentCommandResult {
                request_id: received,
                result,
            } = message
            else {
                continue;
            };
            if received != request_id {
                continue;
            }
            return match result {
                AgentCommandResult::Ok | AgentCommandResult::Text(_) => Ok(0),
                AgentCommandResult::Error(error) => Err(error),
            };
        }
        Err("Vmux disconnected before the browser request completed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn opening_preserves_the_auth_url_and_waits_for_the_matching_reply() {
        let fixture = tempfile::tempdir().unwrap();
        let socket = fixture.path().join("browser.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let receiver = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_nonblocking(true).unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let mut stream = tokio::net::UnixStream::from_std(stream).unwrap();
                let message =
                    ServiceCodec::read_client(&mut tokio::io::BufReader::new(&mut stream))
                        .await
                        .unwrap()
                        .unwrap();
                let ClientMessage::AgentRequest {
                    request_id,
                    request,
                    ..
                } = message
                else {
                    panic!("browser opening must use the existing request bridge");
                };
                let request = request.decode::<AgentOpenBrowser>().unwrap().unwrap();
                ServiceCodec::write_service(
                    &mut stream,
                    &ServiceMessage::AgentCommandResult {
                        request_id: AgentRequestId::new(),
                        result: AgentCommandResult::Error("unrelated reply".into()),
                    },
                )
                .await
                .unwrap();
                ServiceCodec::write_service(
                    &mut stream,
                    &ServiceMessage::AgentCommandResult {
                        request_id,
                        result: AgentCommandResult::Ok,
                    },
                )
                .await
                .unwrap();
                request
            })
        });
        let url = "https://example.test/auth?state=%2B%2F&challenge=a+b";
        let invocation = CliInvocation {
            command: "browser.open".into(),
            arguments: std::collections::BTreeMap::from([
                ("url".into(), vec![url.into()]),
                ("source".into(), vec!["setup-session".into()]),
            ]),
        };
        assert_eq!(Open::try_from(&invocation).unwrap().send(&socket), Ok(0));
        let received = receiver.join().unwrap();
        assert_eq!(received.url, url);
        assert_eq!(received.source.as_deref(), Some("setup-session"));
    }
}
