use std::io;

pub async fn run(
    anchor: Option<String>,
    profile: Option<String>,
    acp_session: bool,
    acp_terminals: bool,
    run_timeout_secs: u64,
) -> io::Result<()> {
    if let Some(p) = profile
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
    {
        unsafe { std::env::set_var("VMUX_PROFILE", p) };
    }
    let anchor = anchor.and_then(|s| s.parse::<vmux_service::protocol::ProcessId>().ok());
    vmux_mcp::protocol::run_stdio(
        anchor,
        acp_session,
        acp_terminals,
        std::time::Duration::from_secs(run_timeout_secs),
    )
    .await
}
