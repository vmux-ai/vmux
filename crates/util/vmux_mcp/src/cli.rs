use std::time::Duration;

use bevy_app::{App, Plugin};
use vmux_api::protocol::ProcessId;
use vmux_core::cli::{CliAppFuture, CliAppHandler, CliInvocation, CliManifestPlugin, CliResult};

pub struct McpCliPlugin;

impl Plugin for McpCliPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CliManifestPlugin::new(include_str!("cli.ron")));
        app.world_mut().spawn(CliAppHandler {
            command: "mcp",
            run: run_mcp,
        });
    }
}

struct McpCliOptions {
    anchor: Option<ProcessId>,
    profile: Option<String>,
    acp_session: bool,
    acp_terminals: bool,
    run_block_timeout: Duration,
    shell: String,
}

impl TryFrom<&CliInvocation> for McpCliOptions {
    type Error = String;

    fn try_from(invocation: &CliInvocation) -> Result<Self, Self::Error> {
        let anchor = invocation
            .value("anchor")
            .and_then(|value| value.parse::<ProcessId>().ok());
        let profile = invocation
            .value("profile")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let run_timeout_secs = invocation
            .value("run_timeout_secs")
            .unwrap_or("50")
            .parse::<u64>()
            .map_err(|error| format!("invalid --run-timeout-secs: {error}"))?;
        Ok(Self {
            anchor,
            profile,
            acp_session: invocation.flag("acp_session"),
            acp_terminals: invocation.flag("acp_terminals"),
            run_block_timeout: Duration::from_secs(run_timeout_secs),
            shell: invocation.value("shell").unwrap_or_default().to_string(),
        })
    }
}

fn run_mcp(mut app: App, invocation: CliInvocation) -> CliAppFuture {
    Box::pin(async move {
        let options = match McpCliOptions::try_from(&invocation) {
            Ok(options) => options,
            Err(error) => return CliResult(Err(error)),
        };
        if let Some(profile) = options.profile {
            unsafe { std::env::set_var("VMUX_PROFILE", profile) };
        }
        app.add_plugins(crate::protocol::McpPlugin::new(
            options.anchor,
            options.acp_session,
            options.acp_terminals,
            options.run_block_timeout,
            options.shell,
        ));
        CliResult(
            crate::protocol::run_stdio(app)
                .await
                .map(|()| 0)
                .map_err(|error| error.to_string()),
        )
    })
}
