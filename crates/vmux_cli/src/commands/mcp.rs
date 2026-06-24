use std::io;

pub async fn run(anchor: Option<String>, profile: Option<String>) -> io::Result<()> {
    if let Some(p) = profile
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
    {
        unsafe { std::env::set_var("VMUX_PROFILE", p) };
    }
    let anchor = anchor.and_then(|s| s.parse::<vmux_service::protocol::ProcessId>().ok());
    vmux_mcp::protocol::run_stdio(anchor).await
}
