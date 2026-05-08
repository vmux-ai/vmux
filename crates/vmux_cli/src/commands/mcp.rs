use std::io;

pub async fn run() -> io::Result<()> {
    vmux_mcp::protocol::run_stdio().await
}
