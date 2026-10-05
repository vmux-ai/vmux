use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, TryLockError};

#[derive(Clone, Debug, Default)]
pub struct McpCredentialAccess {
    transaction: Arc<Mutex<()>>,
    refresh: Arc<Mutex<()>>,
    revision: Arc<AtomicU64>,
}

impl McpCredentialAccess {
    pub fn read<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _access = self.transaction.lock().map_err(|error| error.to_string())?;
        operation()
    }

    pub fn write<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _access = self.transaction.lock().map_err(|error| error.to_string())?;
        self.revision.fetch_add(1, Ordering::AcqRel);
        let result = operation();
        self.revision.fetch_add(1, Ordering::AcqRel);
        result
    }

    pub fn refresh<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _access = self.refresh.lock().map_err(|error| error.to_string())?;
        operation()
    }

    pub fn stable_revision(&self) -> Result<u64, String> {
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
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let writer_access = access.clone();
        let writer = std::thread::spawn(move || {
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
}
