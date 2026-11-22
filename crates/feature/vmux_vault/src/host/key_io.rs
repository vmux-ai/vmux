use std::sync::mpsc;
use std::thread::JoinHandle;

use bevy::prelude::*;
use parking_lot::Mutex;
use zeroize::Zeroizing;

use crate::storage::keys::KeyWriteReceipt;

pub(super) struct NativeCall<T: Send + 'static> {
    receiver: Mutex<mpsc::Receiver<Result<T, String>>>,
    worker: Option<JoinHandle<()>>,
    result: Option<Result<T, String>>,
    wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
}

impl<T: Send + 'static> NativeCall<T> {
    pub(super) fn spawn(
        wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
        call: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let worker_wake = wake.clone();
        let worker = std::thread::spawn(move || {
            let _ = sender.send(call());
            if let Some(wake) = worker_wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
        });
        Self {
            receiver: Mutex::new(receiver),
            worker: Some(worker),
            result: None,
            wake,
        }
    }

    pub(super) fn poll(&mut self) -> Option<Result<T, String>> {
        if self.result.is_none() {
            match self.receiver.get_mut().try_recv() {
                Ok(result) => self.result = Some(result),
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.result = Some(Err("Vault native key worker failed".to_string()));
                }
            }
        }
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            if let Some(wake) = &self.wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            return None;
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            self.result = Some(Err("Vault native key worker failed".to_string()));
        }
        self.result.take()
    }
}

impl<T: Send + 'static> Drop for NativeCall<T> {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Component)]
pub(super) struct ReadTask(pub(super) NativeCall<Zeroizing<Vec<u8>>>);

#[derive(Component)]
pub(super) struct ObserveTask(pub(super) NativeCall<Option<Zeroizing<Vec<u8>>>>);

#[derive(Component)]
pub(super) struct GenerateTask(pub(super) NativeCall<Zeroizing<Vec<u8>>>);

#[derive(Component)]
pub(super) struct StoreTask(pub(super) NativeCall<KeyWriteReceipt>);
