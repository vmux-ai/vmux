use async_lock::{Semaphore, SemaphoreGuardArc};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, TryLockError};

#[derive(Clone, Debug)]
pub struct McpCredentialAccess {
    transaction: Arc<Mutex<()>>,
    serial: Arc<Semaphore>,
    refresh: Arc<Mutex<()>>,
    revision: Arc<AtomicU64>,
}

impl Default for McpCredentialAccess {
    fn default() -> Self {
        Self {
            transaction: Arc::new(Mutex::new(())),
            serial: Arc::new(Semaphore::new(1)),
            refresh: Arc::new(Mutex::new(())),
            revision: Arc::new(AtomicU64::new(0)),
        }
    }
}

pub struct McpCredentialTransaction {
    access: McpCredentialAccess,
    _guard: SemaphoreGuardArc,
}

impl Drop for McpCredentialTransaction {
    fn drop(&mut self) {
        self.access.revision.fetch_add(1, Ordering::AcqRel);
    }
}

impl McpCredentialAccess {
    pub fn read<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _serial = self.serial.acquire_arc_blocking();
        let _access = self.transaction.lock().map_err(|error| error.to_string())?;
        operation()
    }

    pub fn write<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _serial = self.serial.acquire_arc_blocking();
        let _access = self.transaction.lock().map_err(|error| error.to_string())?;
        self.revision.fetch_add(1, Ordering::AcqRel);
        let result = operation();
        self.revision.fetch_add(1, Ordering::AcqRel);
        result
    }

    pub async fn transaction(&self) -> Result<McpCredentialTransaction, String> {
        let guard = self.serial.acquire_arc().await;
        drop(self.transaction.lock().map_err(|error| error.to_string())?);
        self.revision.fetch_add(1, Ordering::AcqRel);
        Ok(McpCredentialTransaction {
            access: self.clone(),
            _guard: guard,
        })
    }

    pub fn refresh<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _access = self.refresh.lock().map_err(|error| error.to_string())?;
        operation()
    }

    pub fn stable_revision(&self) -> Result<u64, String> {
        let _serial = self.serial.acquire_arc_blocking();
        let _access = self.transaction.lock().map_err(|error| error.to_string())?;
        Ok(self.revision())
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    pub fn with_revision<T>(
        &self,
        revision: u64,
        operation: impl FnOnce() -> T,
    ) -> Result<Option<T>, String> {
        let Some(_serial) = self.serial.try_acquire_arc() else {
            return Ok(None);
        };
        let _access = match self.transaction.try_lock() {
            Ok(access) => access,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(error)) => return Err(error.to_string()),
        };
        if self.revision() != revision || !revision.is_multiple_of(2) {
            return Ok(None);
        }
        Ok(Some(operation()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;

    static TEST_ACCESS: Mutex<()> = Mutex::new(());

    #[test]
    fn writes_invalidate_prepared_launches() {
        let _test = TEST_ACCESS.lock().unwrap();
        let access = McpCredentialAccess::default();
        let revision = access.stable_revision().unwrap();
        assert_eq!(
            access.with_revision(revision, || "current").unwrap(),
            Some("current")
        );

        access.write(|| Ok(())).unwrap();

        assert_eq!(access.with_revision(revision, || "stale").unwrap(), None);

        let revision = access.stable_revision().unwrap();
        let result: Result<(), String> = access.write(|| Err("possibly changed".to_string()));
        assert!(result.is_err());
        assert_eq!(access.with_revision(revision, || "current").unwrap(), None);
    }

    #[test]
    fn launch_validation_does_not_wait_for_writes() {
        let _test = TEST_ACCESS.lock().unwrap();
        let access = McpCredentialAccess::default();
        let revision = access.stable_revision().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let writer_access = access.clone();
        let writer = thread::spawn(move || {
            writer_access
                .write(|| {
                    started_tx.send(()).unwrap();
                    finish_rx.recv().unwrap();
                    Ok(())
                })
                .unwrap();
        });
        started_rx.recv().unwrap();

        assert_eq!(access.with_revision(revision, || "stale").unwrap(), None);

        finish_tx.send(()).unwrap();
        writer.join().unwrap();
    }
    #[test]
    fn owned_transaction_retains_exclusion_and_invalidates_prior_revisions_until_release() {
        let access = McpCredentialAccess::default();
        let before = access.stable_revision().unwrap();
        let transaction = thread::scope(|threads| {
            threads
                .spawn(|| {
                    let mut future = std::pin::pin!(access.transaction());
                    let waker = std::task::Waker::noop();
                    let mut context = std::task::Context::from_waker(waker);
                    match std::future::Future::poll(future.as_mut(), &mut context) {
                        std::task::Poll::Ready(result) => result.unwrap(),
                        std::task::Poll::Pending => {
                            panic!("uncontended transaction was not admitted")
                        }
                    }
                })
                .join()
                .unwrap()
        });
        assert_eq!(access.with_revision(before, || "stale").unwrap(), None);
        let (started, entered) = mpsc::channel();
        let (finished, result) = mpsc::channel();
        let reader_access = access.clone();
        let reader = thread::spawn(move || {
            started.send(()).unwrap();
            let value = reader_access.read(|| Ok("released"));
            finished.send(value).unwrap();
        });
        entered.recv().unwrap();
        assert!(
            result
                .recv_timeout(std::time::Duration::from_millis(30))
                .is_err()
        );
        drop(transaction);
        assert_eq!(
            result
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap()
                .unwrap(),
            "released"
        );
        reader.join().unwrap();
        let after = access.stable_revision().unwrap();
        assert!(after > before);
        assert_eq!(access.with_revision(before, || "old").unwrap(), None);
        assert_eq!(
            access.with_revision(after, || "current").unwrap(),
            Some("current")
        );
    }
}
