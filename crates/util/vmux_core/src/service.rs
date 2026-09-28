use std::collections::HashMap;
use std::hash::Hash;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use vmux_profile::{active_profile_name, build_profile, shared_data_dir};

#[cfg(host)]
use bevy::prelude::*;
#[cfg(host)]
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
#[cfg(host)]
use tokio::net::UnixStream;
#[cfg(host)]
use tokio::sync::Mutex as TokioMutex;
#[cfg(host)]
use tokio::sync::oneshot;
#[cfg(host)]
use vmux_api::protocol::{ClientMessage, ServiceMessage};
#[cfg(host)]
use vmux_transport::framing::LengthPrefixed;

#[cfg(host)]
const CODEC: LengthPrefixed = LengthPrefixed::new(64 * 1024 * 1024);

#[cfg(host)]
#[derive(Clone, Message)]
pub struct ServiceRequest(pub vmux_api::protocol::ClientMessage);

#[cfg(host)]
#[derive(Clone, Message)]
pub struct ServiceInbound(pub vmux_api::protocol::ServiceMessage);

#[cfg(host)]
#[derive(Component)]
pub struct ServiceConnected;

#[cfg(host)]
#[derive(Component, Clone, Debug)]
pub struct ServiceUnavailable(pub String);

#[cfg(host)]
pub struct PendingRequests<K, V> {
    entries: Arc<TokioMutex<HashMap<K, oneshot::Sender<V>>>>,
}

#[cfg(host)]
impl<K, V> Clone for PendingRequests<K, V> {
    fn clone(&self) -> Self {
        Self {
            entries: Arc::clone(&self.entries),
        }
    }
}

#[cfg(host)]
impl<K, V> Default for PendingRequests<K, V> {
    fn default() -> Self {
        Self {
            entries: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }
}

#[cfg(host)]
impl<K, V> PendingRequests<K, V>
where
    K: Copy + Eq + Hash,
{
    pub async fn request(
        &self,
        id: K,
        timeout: Duration,
        publish: impl FnOnce() -> bool,
        unavailable: &'static str,
        timed_out: &'static str,
    ) -> Result<V, String> {
        let (sender, receiver) = oneshot::channel();
        self.entries.lock().await.insert(id, sender);
        if !publish() {
            self.entries.lock().await.remove(&id);
            return Err(unavailable.to_string());
        }
        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(response)) => Ok(response),
            _ => {
                self.entries.lock().await.remove(&id);
                Err(timed_out.to_string())
            }
        }
    }

    pub async fn resolve(&self, id: K, response: V) -> bool {
        let Some(sender) = self.entries.lock().await.remove(&id) else {
            return false;
        };
        sender.send(response).is_ok()
    }
}

#[cfg(host)]
pub struct ServiceConnection {
    reader: TokioMutex<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: TokioMutex<tokio::net::unix::OwnedWriteHalf>,
}

#[cfg(host)]
impl ServiceConnection {
    pub async fn connect() -> std::io::Result<Self> {
        let stream = UnixStream::connect(ServicePaths::current().socket()).await?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: TokioMutex::new(BufReader::new(reader)),
            writer: TokioMutex::new(writer),
        })
    }

    pub async fn send(&self, message: &ClientMessage) -> std::io::Result<()> {
        let mut writer = self.writer.lock().await;
        write_client_message(&mut *writer, message).await
    }

    pub async fn recv(&self) -> std::io::Result<Option<ServiceMessage>> {
        let mut reader = self.reader.lock().await;
        read_service_message(&mut *reader).await
    }
}

#[cfg(host)]
pub async fn write_raw_frame<W>(writer: &mut W, data: &[u8]) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    CODEC.write(writer, data).await
}

#[cfg(host)]
pub async fn read_raw_frame<R>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>>
where
    R: AsyncReadExt + Unpin,
{
    CODEC.read(reader).await
}

#[cfg(host)]
pub fn write_raw_frame_blocking<W: std::io::Write>(
    writer: &mut W,
    data: &[u8],
) -> std::io::Result<()> {
    CODEC.write_blocking(writer, data)
}

#[cfg(host)]
pub fn read_raw_frame_blocking<R: std::io::Read>(
    reader: &mut R,
) -> std::io::Result<Option<Vec<u8>>> {
    CODEC.read_blocking(reader)
}

#[cfg(host)]
pub async fn write_client_message<W>(writer: &mut W, message: &ClientMessage) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(message)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    write_raw_frame(writer, &bytes).await
}

#[cfg(host)]
pub async fn write_service_message<W>(
    writer: &mut W,
    message: &ServiceMessage,
) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(message)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    write_raw_frame(writer, &bytes).await
}

#[cfg(host)]
pub async fn read_client_message<R>(reader: &mut R) -> std::io::Result<Option<ClientMessage>>
where
    R: AsyncReadExt + Unpin,
{
    let Some(bytes) = read_raw_frame(reader).await? else {
        return Ok(None);
    };
    rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes)
        .map(Some)
        .map_err(|error| std::io::Error::other(error.to_string()))
}

#[cfg(host)]
pub async fn read_service_message<R>(reader: &mut R) -> std::io::Result<Option<ServiceMessage>>
where
    R: AsyncReadExt + Unpin,
{
    let Some(bytes) = read_raw_frame(reader).await? else {
        return Ok(None);
    };
    rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes)
        .map(Some)
        .map_err(|error| std::io::Error::other(error.to_string()))
}

#[cfg(host)]
pub fn write_client_message_blocking<W: std::io::Write>(
    writer: &mut W,
    message: &ClientMessage,
) -> std::io::Result<()> {
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(message)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    write_raw_frame_blocking(writer, &bytes)
}

#[cfg(host)]
pub fn read_service_message_blocking<R: std::io::Read>(
    reader: &mut R,
) -> std::io::Result<Option<ServiceMessage>> {
    let Some(bytes) = read_raw_frame_blocking(reader)? else {
        return Ok(None);
    };
    rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes)
        .map(Some)
        .map_err(|error| std::io::Error::other(error.to_string()))
}

#[derive(Clone, Debug)]
pub struct ServicePaths {
    build: &'static str,
    profile: String,
}

impl ServicePaths {
    pub fn current() -> Self {
        Self {
            build: build_profile(),
            profile: active_profile_name(),
        }
    }

    pub fn build_profile() -> &'static str {
        build_profile()
    }

    pub fn dir() -> PathBuf {
        shared_data_dir().join("services")
    }

    pub fn log_dir() -> PathBuf {
        shared_data_dir().join("logs")
    }

    pub fn shell_integration_dir() -> PathBuf {
        shared_data_dir().join("shell-integration")
    }

    pub fn socket(&self) -> PathBuf {
        self.runtime_file("sock")
    }

    pub fn pid(&self) -> PathBuf {
        self.runtime_file("pid")
    }

    pub fn identity(&self) -> PathBuf {
        self.runtime_file("identity")
    }

    pub fn log(&self) -> PathBuf {
        Self::log_dir().join(self.file_name("log"))
    }

    pub fn current_log(&self) -> PathBuf {
        let date = chrono::Utc::now().format("%Y-%m-%d");
        Self::log_dir().join(format!("{}.{date}.log", self.stem()))
    }

    pub fn remote(&self) -> RemotePaths {
        RemotePaths {
            service: self.clone(),
        }
    }

    fn stem(&self) -> String {
        if self.profile == "personal" {
            format!("vmux-{}", self.build)
        } else {
            format!("vmux-{}-{}", self.build, self.profile)
        }
    }

    fn file_name(&self, ext: &str) -> String {
        format!("{}.{ext}", self.stem())
    }

    fn runtime_file(&self, ext: &str) -> PathBuf {
        Self::dir().join(self.file_name(ext))
    }
}

#[derive(Clone, Debug)]
pub struct RemotePaths {
    service: ServicePaths,
}

impl RemotePaths {
    pub fn current() -> Self {
        ServicePaths::current().remote()
    }

    pub fn relay_token(&self) -> PathBuf {
        self.service.runtime_file("remote-token")
    }

    pub fn authorizations(&self) -> PathBuf {
        self.service.runtime_file("remote-authorizations")
    }

    pub fn state(&self) -> PathBuf {
        self.service.runtime_file("remote-state")
    }

    pub fn certificate(&self) -> PathBuf {
        self.service.runtime_file("remote-cert")
    }

    pub fn key(&self) -> PathBuf {
        self.service.runtime_file("remote-key")
    }

    pub fn fingerprint(&self) -> PathBuf {
        self.service.runtime_file("remote-fingerprint")
    }

    pub fn relay_device(&self) -> PathBuf {
        self.service.runtime_file("remote-device")
    }

    pub fn relay_url(&self) -> PathBuf {
        self.service.runtime_file("remote-relay-url")
    }

    pub fn relay_registration(&self) -> PathBuf {
        self.service.runtime_file("remote-relay-registration")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_profile_is_compile_env() {
        let p = ServicePaths::build_profile();
        assert!(!p.is_empty());
        assert!(matches!(p, "release" | "local" | "dev"));
    }

    #[test]
    fn socket_path_includes_profile_suffix() {
        let s = ServicePaths::current().socket();
        let name = s.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("vmux-"));
        assert!(name.ends_with(".sock"));
        assert!(name.contains(ServicePaths::build_profile()));
    }

    #[test]
    fn remote_token_uses_profile_file_name() {
        let path = RemotePaths::current().relay_token();
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("remote-token")
        );
    }

    #[test]
    fn remote_authorizations_use_profile_file_name() {
        let path = RemotePaths::current().authorizations();
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("remote-authorizations")
        );
    }

    #[test]
    fn profile_file_name_suffixes_only_non_personal() {
        let personal = ServicePaths {
            build: "dev",
            profile: "personal".to_string(),
        };
        let test_dev = ServicePaths {
            build: "dev",
            profile: "test".to_string(),
        };
        let test_release = ServicePaths {
            build: "release",
            profile: "test".to_string(),
        };

        assert_eq!(personal.file_name("sock"), "vmux-dev.sock");
        assert_eq!(test_dev.file_name("sock"), "vmux-dev-test.sock");
        assert_eq!(test_release.file_name("log"), "vmux-release-test.log");
    }

    #[test]
    fn pid_log_identity_paths_share_profile_suffix() {
        let paths = ServicePaths::current();
        let suffix = format!("vmux-{}", ServicePaths::build_profile());
        for p in [paths.pid(), paths.identity(), paths.log()] {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            assert!(
                name.starts_with(&suffix),
                "expected {name} to start with {suffix}"
            );
        }
    }

    #[test]
    fn service_and_log_dirs_nest_under_profile_data_dir() {
        let base = shared_data_dir();
        assert_eq!(ServicePaths::dir(), base.join("services"));
        assert_eq!(ServicePaths::log_dir(), base.join("logs"));
    }

    #[test]
    fn log_path_lives_in_log_dir_not_service_dir() {
        let paths = ServicePaths::current();
        let p = paths.log();
        assert_eq!(p.parent().unwrap(), ServicePaths::log_dir());
        assert_ne!(p.parent().unwrap(), ServicePaths::dir());
        assert_eq!(
            p.file_name().unwrap().to_string_lossy(),
            paths.file_name("log")
        );
    }

    #[test]
    fn current_log_file_lives_in_log_dir_with_profile_and_date() {
        let p = ServicePaths::current().current_log();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with(&format!("vmux-{}.", ServicePaths::build_profile())),
            "got {name}"
        );
        assert!(name.ends_with(".log"), "got {name}");
        assert_eq!(p.parent().unwrap(), ServicePaths::log_dir());
        assert!(
            ServicePaths::log_dir().ends_with("logs"),
            "got {}",
            ServicePaths::log_dir().display()
        );
    }
}
