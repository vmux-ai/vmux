use std::time::Duration;

use bevy::app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_core::cli::{CliInvocation, CliResult};
use vmux_core::host::manifest::FeaturePlugin;

#[cfg(target_os = "macos")]
use super::{DaemonBinary, LaunchAgent};
use vmux_core::service::ServicePaths;

pub struct ServiceCliPlugin;

impl Plugin for ServiceCliPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .add_systems(Update, route)
            .add_systems(
                Update,
                (
                    status, start, stop, restart, logs, install, uninstall, pair, list, revoke,
                )
                    .after(route),
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

fn route(
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

fn status(requests: Query<Entity, Added<ServiceStatusRequest>>, mut commands: Commands) {
    for entity in &requests {
        let (info, live) = StatusInfo::current();
        print!("{}", info.render());
        commands
            .entity(entity)
            .insert(CliResult::from_io(Ok(if live { 0 } else { 1 })));
    }
}

fn start(requests: Query<Entity, Added<ServiceStartRequest>>, mut commands: Commands) {
    for entity in &requests {
        #[cfg(target_os = "macos")]
        let result = DaemonBinary::current()
            .and_then(|binary| LaunchAgent::current().ensure_running(binary.path()))
            .map(|_| 0);
        #[cfg(not(target_os = "macos"))]
        let result = {
            eprintln!("vmux service: launchd commands are macOS-only");
            Ok(2)
        };
        commands.entity(entity).insert(CliResult::from_io(result));
    }
}

fn stop(requests: Query<Entity, Added<ServiceStopRequest>>, mut commands: Commands) {
    for entity in &requests {
        #[cfg(target_os = "macos")]
        let result = LaunchAgent::current().bootout().map(|_| 0);
        #[cfg(not(target_os = "macos"))]
        let result = {
            eprintln!("vmux service: launchd commands are macOS-only");
            Ok(2)
        };
        commands.entity(entity).insert(CliResult::from_io(result));
    }
}

fn restart(requests: Query<Entity, Added<ServiceRestartRequest>>, mut commands: Commands) {
    for entity in &requests {
        #[cfg(target_os = "macos")]
        let result = DaemonBinary::current().and_then(|binary| {
            let agent = LaunchAgent::current();
            let _ = agent.bootout();
            agent.ensure_running(binary.path())?;
            Ok(0)
        });
        #[cfg(not(target_os = "macos"))]
        let result = {
            eprintln!("vmux service: launchd commands are macOS-only");
            Ok(2)
        };
        commands.entity(entity).insert(CliResult::from_io(result));
    }
}

fn logs(
    requests: Query<(Entity, &ServiceLogsRequest), Added<ServiceLogsRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        use std::os::unix::process::CommandExt;
        let mut command = std::process::Command::new("tail");
        if request.0 {
            command.arg("-f");
        }
        command.arg(ServicePaths::current().current_log());
        commands
            .entity(entity)
            .insert(CliResult::from_io(Err(command.exec())));
    }
}

fn install(requests: Query<Entity, Added<ServiceInstallRequest>>, mut commands: Commands) {
    for entity in &requests {
        #[cfg(target_os = "macos")]
        let result = DaemonBinary::current().and_then(|binary| {
            let plist = LaunchAgent::current().install(binary.path())?;
            println!("installed: {}", plist.display());
            Ok(0)
        });
        #[cfg(not(target_os = "macos"))]
        let result = {
            eprintln!("vmux service: launchd commands are macOS-only");
            Ok(2)
        };
        commands.entity(entity).insert(CliResult::from_io(result));
    }
}

fn uninstall(requests: Query<Entity, Added<ServiceUninstallRequest>>, mut commands: Commands) {
    for entity in &requests {
        #[cfg(target_os = "macos")]
        let result = {
            let agent = LaunchAgent::current();
            agent.uninstall().map(|_| {
                println!("uninstalled: {}", agent.plist_path().display());
                0
            })
        };
        #[cfg(not(target_os = "macos"))]
        let result = {
            eprintln!("vmux service: launchd commands are macOS-only");
            Ok(2)
        };
        commands.entity(entity).insert(CliResult::from_io(result));
    }
}

fn pair(
    requests: Query<(Entity, &RemotePairRequest), Added<RemotePairRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        #[cfg(target_os = "macos")]
        let result = (|| {
            let agent = LaunchAgent::current();
            if request.reset {
                let remote = super::RemotePaths::current();
                let _ = agent.bootout();
                let _ = std::fs::remove_file(remote.relay_token());
                let _ = crate::RemoteAuthorizationStore::current().reset();
                let _ = std::fs::remove_file(remote.relay_device());
                let _ = std::fs::remove_file(remote.relay_url());
                let _ = std::fs::remove_file(remote.relay_registration());
            }
            agent.ensure_running(DaemonBinary::current()?.path())?;
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
        })();
        #[cfg(not(target_os = "macos"))]
        let result = {
            let _ = request;
            eprintln!("vmux remote is currently macOS-only");
            Ok(2)
        };
        commands.entity(entity).insert(CliResult::from_io(result));
    }
}

fn list(requests: Query<Entity, Added<RemoteListRequest>>, mut commands: Commands) {
    for entity in &requests {
        #[cfg(target_os = "macos")]
        let result = crate::RemoteAuthorizationStore::current()
            .devices()
            .map(|devices| {
                for device in devices {
                    println!("{}\t{}", device.id.as_str(), device.authorized_at_unix);
                }
                0
            });
        #[cfg(not(target_os = "macos"))]
        let result = {
            eprintln!("vmux remote is currently macOS-only");
            Ok(2)
        };
        commands.entity(entity).insert(CliResult::from_io(result));
    }
}

fn revoke(
    requests: Query<(Entity, &RemoteRevokeRequest), Added<RemoteRevokeRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        #[cfg(target_os = "macos")]
        let result = {
            let client_id = vmux_transport::DeviceId::new(&request.0);
            crate::RemoteAuthorizationStore::current()
                .revoke(&client_id)
                .map(|revoked| {
                    if revoked {
                        println!("revoked {}", client_id.as_str());
                        0
                    } else {
                        eprintln!("device not found: {}", client_id.as_str());
                        1
                    }
                })
        };
        #[cfg(not(target_os = "macos"))]
        let result = {
            let _ = request;
            eprintln!("vmux remote is currently macOS-only");
            Ok(2)
        };
        commands.entity(entity).insert(CliResult::from_io(result));
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
    fn current() -> (Self, bool) {
        let live = Self::live();
        let info = Self {
            profile: ServicePaths::build_profile().to_string(),
            pid: Self::pid(),
            uptime: live.map(|(seconds, _)| Duration::from_secs(seconds)),
            socket: ServicePaths::current().socket(),
            identity_short: Self::identity_short(),
            process_count: live.map(|(_, count)| count),
        };
        (info, live.is_some())
    }

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
            self.uptime
                .map(Self::format_uptime)
                .unwrap_or_else(|| "-".into())
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

    fn format_uptime(duration: Duration) -> String {
        let seconds = duration.as_secs();
        let (hours, remaining) = (seconds / 3600, seconds % 3600);
        let (minutes, seconds) = (remaining / 60, remaining % 60);
        if hours > 0 {
            format!("{hours}h {minutes}m {seconds}s")
        } else if minutes > 0 {
            format!("{minutes}m {seconds}s")
        } else {
            format!("{seconds}s")
        }
    }

    fn pid() -> Option<i32> {
        std::fs::read_to_string(ServicePaths::current().pid())
            .ok()
            .and_then(|value| value.trim().parse().ok())
    }

    fn identity_short() -> Option<String> {
        std::fs::read_to_string(ServicePaths::current().identity())
            .ok()
            .map(|value| {
                let mut hash: u64 = 5381;
                for byte in value.trim().bytes() {
                    hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
                }
                let folded = (hash as u32) ^ ((hash >> 32) as u32);
                format!("{folded:08x}")
            })
    }

    fn live() -> Option<(u64, u32)> {
        use vmux_api::protocol::{ClientMessage, ServiceMessage};
        let result = (|| {
            let stream = std::os::unix::net::UnixStream::connect(ServicePaths::current().socket())?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            stream.set_write_timeout(Some(Duration::from_secs(2)))?;
            let mut stream = stream;
            vmux_core::service::write_client_message_blocking(&mut stream, &ClientMessage::Status)?;
            let mut reader = std::io::BufReader::new(&mut stream);
            vmux_core::service::read_service_message_blocking(&mut reader)
        })();
        match result {
            Ok(Some(ServiceMessage::StatusResponse {
                uptime_secs,
                process_count,
            })) => Some((uptime_secs, process_count)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn format_uptime_formats_segments() {
        assert_eq!(StatusInfo::format_uptime(Duration::from_secs(0)), "0s");
        assert_eq!(StatusInfo::format_uptime(Duration::from_secs(45)), "45s");
        assert_eq!(StatusInfo::format_uptime(Duration::from_secs(75)), "1m 15s");
        assert_eq!(
            StatusInfo::format_uptime(Duration::from_secs(3601)),
            "1h 0m 1s"
        );
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
