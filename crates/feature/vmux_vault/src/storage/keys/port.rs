use std::sync::mpsc;

use bevy::prelude::*;
use parking_lot::Mutex;
use zeroize::Zeroizing;

use super::KeyStore;

#[derive(Component, Clone)]
pub(crate) struct KeyPort {
    pub(crate) origin: KeyOrigin,
    load: mpsc::Sender<LoadKeyRequest>,
    observe: mpsc::Sender<ObserveKeyRequest>,
    pub(crate) wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
}

impl KeyPort {
    pub(crate) fn new(
        origin: KeyOrigin,
        wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
    ) -> (Self, KeyIngress) {
        let (load, load_rx) = mpsc::channel();
        let (observe, observe_rx) = mpsc::channel();
        (
            Self {
                origin,
                load,
                observe,
                wake,
            },
            KeyIngress {
                load: Mutex::new(load_rx),
                observe: Mutex::new(observe_rx),
            },
        )
    }

    fn notify(&self) {
        if let Some(wake) = &self.wake {
            let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
    }

    pub(crate) fn observe(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        let (reply, response) = mpsc::channel();
        self.observe
            .send(ObserveKeyRequest {
                origin: self.origin,
                vault_id: vault_id.to_string(),
                reply,
            })
            .map_err(|_| "Vault key operation retired".to_string())?;
        self.notify();
        response
            .recv()
            .map_err(|_| "Vault key operation retired".to_string())?
    }
}

impl KeyPort {
    pub(crate) fn load(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        let (reply, response) = mpsc::channel();
        self.load
            .send(LoadKeyRequest {
                origin: self.origin,
                vault_id: vault_id.to_string(),
                reply,
            })
            .map_err(|_| "Vault key operation retired".to_string())?;
        self.notify();
        response
            .recv()
            .map_err(|_| "Vault key operation retired".to_string())?
    }
}

#[derive(Component)]
pub(crate) struct KeyIngress {
    pub(crate) load: Mutex<mpsc::Receiver<LoadKeyRequest>>,
    pub(crate) observe: Mutex<mpsc::Receiver<ObserveKeyRequest>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyOrigin {
    pub(crate) operation: Entity,
    pub(crate) namespace: Entity,
    pub(crate) generation: u64,
    pub(crate) admission: u64,
}

pub(crate) struct LoadKeyRequest {
    pub(crate) origin: KeyOrigin,
    pub(crate) vault_id: String,
    pub(crate) reply: mpsc::Sender<Result<Zeroizing<Vec<u8>>, String>>,
}

pub(crate) struct ObserveKeyRequest {
    pub(crate) origin: KeyOrigin,
    pub(crate) vault_id: String,
    pub(crate) reply: mpsc::Sender<Result<Zeroizing<Vec<u8>>, String>>,
}

pub(crate) struct KeyObservation<'a>(&'a KeyPort);

impl KeyPort {
    pub(crate) fn observation(&self) -> KeyObservation<'_> {
        KeyObservation(self)
    }
}

impl KeyStore for KeyObservation<'_> {
    fn load(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        self.0.observe(vault_id)
    }
}

impl KeyStore for KeyPort {
    fn load(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        KeyPort::load(self, vault_id)
    }
}
