//! ACP host (Agent Client Protocol). vmux implements the `Client` role: external coding
//! agents run as spawned subprocesses driven over JSON-RPC, surfaced through vmux's native
//! panes.

#[allow(dead_code, unused_variables, unused_imports)]
mod api_lock {
    use agent_client_protocol::schema::ProtocolVersion;
    use agent_client_protocol::schema::v1::*;
    use agent_client_protocol::{Client, ConnectTo};

    async fn shape<T: ConnectTo<Client>>(transport: T) -> agent_client_protocol::Result<()> {
        Client
            .builder()
            .on_receive_notification(
                async move |_n: SessionNotification, _cx| Ok(()),
                agent_client_protocol::on_receive_notification!(),
            )
            .connect_with(transport, async move |cx| {
                let _init = cx
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                let s = cx
                    .send_request(NewSessionRequest::new(std::path::PathBuf::from("/")))
                    .block_task()
                    .await?;
                let _resp = cx
                    .send_request(PromptRequest::new(
                        s.session_id.clone(),
                        vec![ContentBlock::Text(TextContent::new("hi"))],
                    ))
                    .block_task()
                    .await?;
                Ok(())
            })
            .await
    }
}
