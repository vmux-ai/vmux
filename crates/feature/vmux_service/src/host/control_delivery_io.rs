use bevy_ecs::prelude::Component;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use vmux_api::control::{
    ControlHello, ControlReady, CredentialOffer, CredentialReply, CredentialWithdraw,
};
use vmux_api::protocol::{ClientMessage, ServiceMessage};
use vmux_ecs::{Job, blocking::Lane};
use vmux_transport::service::ServiceConnection;
use vmux_transport::service_peer::CodeRequirement;

pub(super) struct Connection {
    runtime: Option<tokio::runtime::Runtime>,
    service: Option<ServiceConnection>,
}

impl Connection {
    fn exchange(
        &self,
        message: ClientMessage,
        timeout: Duration,
    ) -> Result<ServiceMessage, String> {
        let runtime = self.runtime.as_ref().ok_or("Control runtime retired")?;
        let service = self.service.as_ref().ok_or("Control service retired")?;
        runtime.block_on(async {
            tokio::time::timeout(timeout, async {
                service.send(&message).await?;
                service
                    .recv()
                    .await?
                    .ok_or_else(|| std::io::Error::other("Control service disconnected"))
            })
            .await
            .map_err(|_| "Control exchange timed out".to_string())?
            .map_err(|error| error.to_string())
        })
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.service.take();
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

#[derive(Clone, Component)]
#[component(immutable)]
pub(super) struct Connect {
    pub socket: PathBuf,
    pub requirement: CodeRequirement,
    pub uid: u32,
    pub timeout: Duration,
}

impl Job for Connect {
    type Output = Result<Arc<Connection>, String>;

    fn run(self) -> Self::Output {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let service = runtime.block_on(async {
            tokio::time::timeout(
                self.timeout,
                ServiceConnection::connect_checked(&self.socket, &self.requirement, self.uid),
            )
            .await
            .map_err(|_| "Control connect timed out".to_string())?
            .map_err(|error| error.to_string())
        })?;
        Ok(Arc::new(Connection {
            runtime: Some(runtime),
            service: Some(service),
        }))
    }

    fn lane(&self) -> Lane {
        Lane::Long
    }
}

#[derive(Clone, Component)]
#[component(immutable)]
pub(super) struct Hello {
    pub connection: Arc<Connection>,
    pub request: ControlHello,
    pub timeout: Duration,
}

impl Job for Hello {
    type Output = Result<ControlReady, String>;

    fn run(self) -> Self::Output {
        match self
            .connection
            .exchange(ClientMessage::ControlHello(self.request), self.timeout)?
        {
            ServiceMessage::ControlReady(reply) => Ok(reply),
            _ => Err("unexpected Control hello response".into()),
        }
    }

    fn lane(&self) -> Lane {
        Lane::Long
    }
}

#[derive(Clone, Component)]
#[component(immutable)]
pub(super) struct Offer {
    pub connection: Arc<Connection>,
    pub request: CredentialOffer,
    pub timeout: Duration,
}

impl Job for Offer {
    type Output = Result<CredentialReply, String>;

    fn run(self) -> Self::Output {
        match self
            .connection
            .exchange(ClientMessage::CredentialOffer(self.request), self.timeout)?
        {
            ServiceMessage::CredentialReply(reply) => Ok(reply),
            _ => Err("unexpected Control credential response".into()),
        }
    }

    fn lane(&self) -> Lane {
        Lane::Long
    }
}

#[derive(Clone, Component)]
#[component(immutable)]
pub(super) struct Withdraw {
    pub connection: Arc<Connection>,
    pub request: CredentialWithdraw,
    pub timeout: Duration,
}

impl Job for Withdraw {
    type Output = Result<CredentialReply, String>;

    fn run(self) -> Self::Output {
        match self.connection.exchange(
            ClientMessage::CredentialWithdraw(self.request),
            self.timeout,
        )? {
            ServiceMessage::CredentialReply(reply) => Ok(reply),
            _ => Err("unexpected Control withdrawal response".into()),
        }
    }

    fn lane(&self) -> Lane {
        Lane::Long
    }
}
