#[tokio::main]
async fn main() -> std::io::Result<()> {
    vmux_mcp::protocol::run_stdio().await
}
