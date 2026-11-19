use bevy_ecs::prelude::Component;
use tokio::sync::{mpsc, oneshot};
use vmux_api::control::{
    ControlHello, ControlReady, CredentialOffer, CredentialReply, CredentialWithdraw,
};
use vmux_api::protocol::{ClientMessage, ServiceMessage};
use vmux_transport::service::{RemoteFuture, ServiceProtocolConnection, ServiceProtocolDriver};
use vmux_transport::service_peer::LocalPeer;

#[derive(Component)]
pub(super) struct HelloInput {
    pub client: Option<u64>,
    pub request: Option<ControlHello>,
    pub reply: Option<oneshot::Sender<ControlReady>>,
}

#[derive(Component)]
pub(super) struct OfferInput {
    pub client: Option<u64>,
    pub request: Option<CredentialOffer>,
    pub reply: Option<oneshot::Sender<CredentialReply>>,
}

#[derive(Component)]
pub(super) struct WithdrawInput {
    pub client: Option<u64>,
    pub request: Option<CredentialWithdraw>,
    pub reply: Option<oneshot::Sender<CredentialReply>>,
}

#[derive(Component)]
pub(super) struct Inbox {
    pub hello: mpsc::Receiver<HelloInput>,
    pub offer: mpsc::Receiver<OfferInput>,
    pub withdraw: mpsc::Receiver<WithdrawInput>,
}

#[derive(Clone)]
pub(super) struct Driver {
    hello: mpsc::Sender<HelloInput>,
    offer: mpsc::Sender<OfferInput>,
    withdraw: mpsc::Sender<WithdrawInput>,
    wake: mpsc::UnboundedSender<()>,
}

impl Driver {
    pub(super) fn new(wake: mpsc::UnboundedSender<()>) -> (Self, Inbox) {
        let (hello, hellos) = mpsc::channel(16);
        let (offer, offers) = mpsc::channel(16);
        let (withdraw, withdrawals) = mpsc::channel(16);
        (
            Self {
                hello,
                offer,
                withdraw,
                wake,
            },
            Inbox {
                hello: hellos,
                offer: offers,
                withdraw: withdrawals,
            },
        )
    }
}

impl ServiceProtocolDriver for Driver {
    fn connect_local(
        &self,
        client: u64,
        peer: &LocalPeer,
        admitted: bool,
        outbound: mpsc::UnboundedSender<ServiceMessage>,
    ) -> Box<dyn ServiceProtocolConnection> {
        let _ = (peer, admitted);
        Box::new(Connection {
            client: Some(client),
            driver: self.clone(),
            outbound,
        })
    }

    fn connect(
        &self,
        outbound: mpsc::UnboundedSender<ServiceMessage>,
    ) -> Box<dyn ServiceProtocolConnection> {
        Box::new(Connection {
            client: None,
            driver: self.clone(),
            outbound,
        })
    }
}

struct Connection {
    client: Option<u64>,
    driver: Driver,
    outbound: mpsc::UnboundedSender<ServiceMessage>,
}

impl ServiceProtocolConnection for Connection {
    fn dispatch(&mut self, message: ClientMessage) -> RemoteFuture<'_, Result<(), ClientMessage>> {
        Box::pin(async move {
            match message {
                ClientMessage::ControlHello(request) => {
                    let (reply, result) = oneshot::channel();
                    let input = HelloInput {
                        client: self.client,
                        request: Some(request),
                        reply: Some(reply),
                    };
                    if self.driver.hello.send(input).await.is_ok() {
                        let _ = self.driver.wake.send(());
                        if let Ok(reply) = result.await {
                            let _ = self.outbound.send(ServiceMessage::ControlReady(reply));
                        }
                    }
                }
                ClientMessage::CredentialOffer(request) => {
                    let (reply, result) = oneshot::channel();
                    let input = OfferInput {
                        client: self.client,
                        request: Some(request),
                        reply: Some(reply),
                    };
                    if self.driver.offer.send(input).await.is_ok() {
                        let _ = self.driver.wake.send(());
                        if let Ok(reply) = result.await {
                            let _ = self.outbound.send(ServiceMessage::CredentialReply(reply));
                        }
                    }
                }
                ClientMessage::CredentialWithdraw(request) => {
                    let (reply, result) = oneshot::channel();
                    let input = WithdrawInput {
                        client: self.client,
                        request: Some(request),
                        reply: Some(reply),
                    };
                    if self.driver.withdraw.send(input).await.is_ok() {
                        let _ = self.driver.wake.send(());
                        if let Ok(reply) = result.await {
                            let _ = self.outbound.send(ServiceMessage::CredentialReply(reply));
                        }
                    }
                }
                other => return Err(other),
            }
            Ok(())
        })
    }
}
