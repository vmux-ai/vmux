#[cfg(target_os = "macos")]
use std::path::Path;
use std::time::Duration;

use bevy::app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_core::cli::{CliInvocation, CliManifestPlugin, CliResult};

#[cfg(target_os = "macos")]
use super::LaunchAgent;
use vmux_core::service::ServicePaths;

pub struct ServiceCliPlugin;

impl Plugin for ServiceCliPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CliManifestPlugin::new(include_str!("cli.ron")))
            .add_systems(
                Update,
                (route_service_cli, execute_service_cli, execute_remote_cli).chain(),
            );
    }
}

#[derive(Component)]
struct ServiceStatusRequest;

#[derive(Component)]
struct ServiceStartRequest;

#[derive(Component)]
struct ServiceStopRequest;

#[derive(Component)]
struct ServiceRestartRequest;

#[derive(Component)]
struct ServiceLogsRequest(bool);

#[derive(Component)]
struct ServiceInstallRequest;

#[derive(Component)]
struct ServiceUninstallRequest;

#[derive(Component)]
struct RemotePairRequest {
    reset: bool,
}

#[derive(Component)]
struct RemoteListRequest;

#[derive(Component)]
struct RemoteRevokeRequest(String);

fn route_service_cli(
    invocations: Query<(Entity, &CliInvocation), Added<CliInvocation>>,
    mut commands: Commands,
) {
    for (entity, invocation) in &invocations {
        let mut entity = commands.entity(entity);
        match invocation.command.as_str() {
            "service.status" => {
                entity.insert(ServiceStatusRequest);
            }
            "service.start" => {
                entity.insert(ServiceStartRequest);
            }
            "service.stop" => {
                entity.insert(ServiceStopRequest);
            }
            "service.restart" => {
                entity.insert(ServiceRestartRequest);
            }
            "service.logs" => {
                entity.insert(ServiceLogsRequest(invocation.flag("follow")));
            }
            "service.install" => {
                entity.insert(ServiceInstallRequest);
            }
            "service.uninstall" => {
                entity.insert(ServiceUninstallRequest);
            }
            "remote.pair" => {
                entity.insert(RemotePairRequest {
                    reset: invocation.flag("reset"),
                });
            }
            "remote.list" => {
                entity.insert(RemoteListRequest);
            }
            "remote.revoke" => {
                if let Some(client_id) = invocation.value("client_id") {
                    entity.insert(RemoteRevokeRequest(client_id.to_string()));
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_service_cli(
    status: Query<Entity, Added<ServiceStatusRequest>>,
    start: Query<Entity, Added<ServiceStartRequest>>,
    stop: Query<Entity, Added<ServiceStopRequest>>,
    restart: Query<Entity, Added<ServiceRestartRequest>>,
    logs: Query<(Entity, &ServiceLogsRequest), Added<ServiceLogsRequest>>,
    install: Query<Entity, Added<ServiceInstallRequest>>,
    uninstall: Query<Entity, Added<ServiceUninstallRequest>>,
    mut commands: Commands,
) {
    for entity in &status {
        commands
            .entity(entity)
            .insert(CliResult::from_io(cmd_status()));
    }
    for entity in &start {
        commands
            .entity(entity)
            .insert(CliResult::from_io(cmd_start_current()));
    }
    for entity in &stop {
        commands
            .entity(entity)
            .insert(CliResult::from_io(cmd_stop_current()));
    }
    for entity in &restart {
        commands
            .entity(entity)
            .insert(CliResult::from_io(cmd_restart_current()));
    }
    for (entity, request) in &logs {
        commands
            .entity(entity)
            .insert(CliResult::from_io(cmd_logs(request.0)));
    }
    for entity in &install {
        commands
            .entity(entity)
            .insert(CliResult::from_io(cmd_install_current()));
    }
    for entity in &uninstall {
        commands
            .entity(entity)
            .insert(CliResult::from_io(cmd_uninstall_current()));
    }
}

fn execute_remote_cli(
    pair: Query<(Entity, &RemotePairRequest), Added<RemotePairRequest>>,
    list: Query<Entity, Added<RemoteListRequest>>,
    revoke: Query<(Entity, &RemoteRevokeRequest), Added<RemoteRevokeRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &pair {
        commands
            .entity(entity)
            .insert(CliResult::from_io(remote_pair(request.reset)));
    }
    for entity in &list {
        commands
            .entity(entity)
            .insert(CliResult::from_io(remote_list()));
    }
    for (entity, request) in &revoke {
        commands
            .entity(entity)
            .insert(CliResult::from_io(remote_revoke(&request.0)));
    }
}

#[derive(Debug, Clone)]
struct StatusInfo {
    pub profile: String,
    pub pid: Option<i32>,
    pub uptime: Option<Duration>,
    pub socket: std::path::PathBuf,
    pub identity_short: Option<String>,
    pub process_count: Option<u32>,
}

impl StatusInfo {
    fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("profile     {}\n", self.profile));
        out.push_str(&format!(
            "pid         {}\n",
            self.pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".into())
        ));
        out.push_str(&format!(
            "uptime      {}\n",
            self.uptime.map(format_uptime).unwrap_or_else(|| "-".into())
        ));
        out.push_str(&format!("socket      {}\n", self.socket.display()));
        out.push_str(&format!(
            "identity    {}\n",
            self.identity_short.clone().unwrap_or_else(|| "-".into())
        ));
        out.push_str(&format!(
            "processes   {}\n",
            self.process_count
                .map(|c| c.to_string())
                .unwrap_or_else(|| "-".into())
        ));
        out
    }
}

fn format_uptime(d: Duration) -> String {
    let s = d.as_secs();
    let (h, rem) = (s / 3600, s % 3600);
    let (m, sec) = (rem / 60, rem % 60);
    if h > 0 {
        format!("{h}h {m}m {sec}s")
    } else if m > 0 {
        format!("{m}m {sec}s")
    } else {
        format!("{sec}s")
    }
}

fn read_pid() -> Option<i32> {
    std::fs::read_to_string(ServicePaths::current().pid())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn read_identity_short() -> Option<String> {
    std::fs::read_to_string(ServicePaths::current().identity())
        .ok()
        .map(|s| {
            let mut hash: u64 = 5381;
            for b in s.trim().bytes() {
                hash = hash.wrapping_mul(33).wrapping_add(b as u64);
            }
            let folded = (hash as u32) ^ ((hash >> 32) as u32);
            format!("{folded:08x}")
        })
}

fn live_status() -> Option<(u64, u32)> {
    live_status_inner().ok().flatten()
}

fn live_status_inner() -> std::io::Result<Option<(u64, u32)>> {
    use vmux_api::protocol::{ClientMessage, ServiceMessage};
    let stream = std::os::unix::net::UnixStream::connect(ServicePaths::current().socket())?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut stream = stream;
    vmux_core::service::write_client_message_blocking(&mut stream, &ClientMessage::Status)?;
    let mut reader = std::io::BufReader::new(&mut stream);
    let msg = vmux_core::service::read_service_message_blocking(&mut reader)?;
    Ok(match msg {
        Some(ServiceMessage::StatusResponse {
            uptime_secs,
            process_count,
        }) => Some((uptime_secs, process_count)),
        _ => None,
    })
}

fn cmd_status() -> std::io::Result<i32> {
    let pid = read_pid();
    let live = live_status();
    let info = StatusInfo {
        profile: ServicePaths::build_profile().to_string(),
        pid,
        uptime: live.map(|(s, _)| Duration::from_secs(s)),
        socket: ServicePaths::current().socket(),
        identity_short: read_identity_short(),
        process_count: live.map(|(_, c)| c),
    };
    print!("{}", info.render());
    Ok(if live.is_some() { 0 } else { 1 })
}

#[cfg(target_os = "macos")]
fn cmd_install(binary_path: &Path) -> std::io::Result<i32> {
    let plist = LaunchAgent::current().install(binary_path)?;
    println!("installed: {}", plist.display());
    Ok(0)
}

#[cfg(target_os = "macos")]
fn cmd_uninstall() -> std::io::Result<i32> {
    let agent = LaunchAgent::current();
    agent.uninstall()?;
    println!("uninstalled: {}", agent.plist_path().display());
    Ok(0)
}

#[cfg(target_os = "macos")]
fn cmd_start(binary_path: &Path) -> std::io::Result<i32> {
    LaunchAgent::current().ensure_running(binary_path)?;
    Ok(0)
}

#[cfg(target_os = "macos")]
fn cmd_stop() -> std::io::Result<i32> {
    LaunchAgent::current().bootout()?;
    Ok(0)
}

#[cfg(target_os = "macos")]
fn cmd_restart(binary_path: &Path) -> std::io::Result<i32> {
    let agent = LaunchAgent::current();
    let _ = agent.bootout();
    agent.ensure_running(binary_path)?;
    Ok(0)
}

fn cmd_logs(follow: bool) -> std::io::Result<i32> {
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new("tail");
    if follow {
        cmd.arg("-f");
    }
    cmd.arg(ServicePaths::current().current_log());
    let err = cmd.exec();
    Err(err)
}

#[cfg(target_os = "macos")]
fn cmd_start_current() -> std::io::Result<i32> {
    cmd_start(super::DaemonBinary::current()?.path())
}

#[cfg(not(target_os = "macos"))]
fn cmd_start_current() -> std::io::Result<i32> {
    unsupported_launchd()
}

#[cfg(target_os = "macos")]
fn cmd_stop_current() -> std::io::Result<i32> {
    cmd_stop()
}

#[cfg(not(target_os = "macos"))]
fn cmd_stop_current() -> std::io::Result<i32> {
    unsupported_launchd()
}

#[cfg(target_os = "macos")]
fn cmd_restart_current() -> std::io::Result<i32> {
    cmd_restart(super::DaemonBinary::current()?.path())
}

#[cfg(not(target_os = "macos"))]
fn cmd_restart_current() -> std::io::Result<i32> {
    unsupported_launchd()
}

#[cfg(target_os = "macos")]
fn cmd_install_current() -> std::io::Result<i32> {
    cmd_install(super::DaemonBinary::current()?.path())
}

#[cfg(not(target_os = "macos"))]
fn cmd_install_current() -> std::io::Result<i32> {
    unsupported_launchd()
}

#[cfg(target_os = "macos")]
fn cmd_uninstall_current() -> std::io::Result<i32> {
    cmd_uninstall()
}

#[cfg(not(target_os = "macos"))]
fn cmd_uninstall_current() -> std::io::Result<i32> {
    unsupported_launchd()
}

#[cfg(not(target_os = "macos"))]
fn unsupported_launchd() -> std::io::Result<i32> {
    eprintln!("vmux service: launchd commands are macOS-only");
    Ok(2)
}

#[cfg(target_os = "macos")]
fn remote_pair(reset: bool) -> std::io::Result<i32> {
    let agent = LaunchAgent::current();
    if reset {
        let remote = super::RemotePaths::current();
        let _ = agent.bootout();
        let _ = std::fs::remove_file(remote.relay_token());
        let _ = crate::RemoteAuthorizationStore::current().reset();
        let _ = std::fs::remove_file(remote.relay_device());
        let _ = std::fs::remove_file(remote.relay_url());
        let _ = std::fs::remove_file(remote.relay_registration());
    }
    agent.ensure_running(super::DaemonBinary::current()?.path())?;
    let relay_token = crate::RelayToken::wait(Duration::from_secs(5))?;
    let pairing_token = crate::RemoteAuthorizationStore::current().pairing_token()?;
    std::fs::write(super::RemotePaths::current().state(), b"enabled\n")?;
    let relay = crate::pairing::Relay::from_env();
    relay.persist()?;
    let pairing_url = relay.wait_for_pairing(
        relay_token.as_str(),
        &pairing_token,
        Duration::from_secs(20),
    )?;
    println!("paste into Vmux Remote: {pairing_url}");
    Ok(0)
}

#[cfg(not(target_os = "macos"))]
fn remote_pair(_reset: bool) -> std::io::Result<i32> {
    eprintln!("vmux remote is currently macOS-only");
    Ok(2)
}

#[cfg(target_os = "macos")]
fn remote_list() -> std::io::Result<i32> {
    for device in crate::RemoteAuthorizationStore::current().devices()? {
        println!("{}\t{}", device.id.as_str(), device.authorized_at_unix);
    }
    Ok(0)
}

#[cfg(not(target_os = "macos"))]
fn remote_list() -> std::io::Result<i32> {
    eprintln!("vmux remote is currently macOS-only");
    Ok(2)
}

#[cfg(target_os = "macos")]
fn remote_revoke(client_id: &str) -> std::io::Result<i32> {
    let client_id = vmux_transport::DeviceId::new(client_id);
    if crate::RemoteAuthorizationStore::current().revoke(&client_id)? {
        println!("revoked {}", client_id.as_str());
        return Ok(0);
    }
    eprintln!("device not found: {}", client_id.as_str());
    Ok(1)
}

#[cfg(not(target_os = "macos"))]
fn remote_revoke(_client_id: &str) -> std::io::Result<i32> {
    eprintln!("vmux remote is currently macOS-only");
    Ok(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn format_uptime_formats_segments() {
        assert_eq!(format_uptime(Duration::from_secs(0)), "0s");
        assert_eq!(format_uptime(Duration::from_secs(45)), "45s");
        assert_eq!(format_uptime(Duration::from_secs(75)), "1m 15s");
        assert_eq!(format_uptime(Duration::from_secs(3601)), "1h 0m 1s");
    }

    #[test]
    fn render_shows_every_field() {
        let info = StatusInfo {
            profile: "dev".into(),
            pid: Some(12345),
            uptime: Some(Duration::from_secs(60)),
            socket: PathBuf::from("/tmp/vmux-dev.sock"),
            identity_short: Some("abcd1234".into()),
            process_count: Some(2),
        };
        let out = info.render();
        assert!(out.contains("profile     dev"));
        assert!(out.contains("pid         12345"));
        assert!(out.contains("uptime      1m 0s"));
        assert!(out.contains("socket      /tmp/vmux-dev.sock"));
        assert!(out.contains("identity    abcd1234"));
        assert!(out.contains("processes   2"));
    }

    #[test]
    fn render_shows_dashes_when_unknown() {
        let info = StatusInfo {
            profile: "dev".into(),
            pid: None,
            uptime: None,
            socket: PathBuf::from("/tmp/vmux-dev.sock"),
            identity_short: None,
            process_count: None,
        };
        let out = info.render();
        assert!(out.contains("pid         -"));
        assert!(out.contains("uptime      -"));
        assert!(out.contains("identity    -"));
        assert!(out.contains("processes   -"));
    }
}
