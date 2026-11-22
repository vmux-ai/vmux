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
    create: mpsc::Sender<CreateKeyRequest>,
    store: mpsc::Sender<StoreKeyRequest>,
    pub(crate) wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
}

impl KeyPort {
    pub(crate) fn new(
        origin: KeyOrigin,
        wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
    ) -> (Self, KeyIngress) {
        let (load, load_rx) = mpsc::channel();
        let (observe, observe_rx) = mpsc::channel();
        let (create, create_rx) = mpsc::channel();
        let (store, store_rx) = mpsc::channel();
        (
            Self {
                origin,
                load,
                observe,
                create,
                store,
                wake,
            },
            KeyIngress {
                load: Mutex::new(load_rx),
                observe: Mutex::new(observe_rx),
                create: Mutex::new(create_rx),
                store: Mutex::new(store_rx),
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

    pub(crate) fn create(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        let (reply, response) = mpsc::channel();
        self.create
            .send(CreateKeyRequest {
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

    pub(crate) fn store(&self, vault_id: &str, key: &[u8]) -> Result<(), String> {
        let (reply, response) = mpsc::channel();
        self.store
            .send(StoreKeyRequest {
                origin: self.origin,
                vault_id: vault_id.to_string(),
                key: Zeroizing::new(key.to_vec()),
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
    pub(crate) create: Mutex<mpsc::Receiver<CreateKeyRequest>>,
    pub(crate) store: Mutex<mpsc::Receiver<StoreKeyRequest>>,
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

pub(crate) struct CreateKeyRequest {
    pub(crate) origin: KeyOrigin,
    pub(crate) vault_id: String,
    pub(crate) reply: mpsc::Sender<Result<Zeroizing<Vec<u8>>, String>>,
}

pub(crate) struct StoreKeyRequest {
    pub(crate) origin: KeyOrigin,
    pub(crate) vault_id: String,
    pub(crate) key: Zeroizing<Vec<u8>>,
    pub(crate) reply: mpsc::Sender<Result<(), String>>,
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

    fn create(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        self.0.create(vault_id)
    }

    fn store(&self, vault_id: &str, key: &[u8]) -> Result<(), String> {
        self.0.store(vault_id, key)
    }
}

impl KeyStore for KeyPort {
    fn load(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        KeyPort::load(self, vault_id)
    }

    fn create(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        KeyPort::create(self, vault_id)
    }

    fn store(&self, vault_id: &str, key: &[u8]) -> Result<(), String> {
        KeyPort::store(self, vault_id, key)
    }
}
