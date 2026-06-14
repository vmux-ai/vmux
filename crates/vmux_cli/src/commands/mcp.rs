use std::io;

pub async fn run(anchor: Option<String>) -> io::Result<()> {
    let anchor = anchor.and_then(|s| s.parse::<vmux_service::protocol::ProcessId>().ok());
    vmux_mcp::protocol::run_stdio(anchor).await
}
