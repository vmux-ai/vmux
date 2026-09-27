use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, TryLockError};

static TRANSACTION_ACCESS: OnceLock<Mutex<()>> = OnceLock::new();
static REFRESH_ACCESS: OnceLock<Mutex<()>> = OnceLock::new();
static REVISION: AtomicU64 = AtomicU64::new(0);

pub struct McpCredentialAccess;

impl McpCredentialAccess {
    pub fn read<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _access = TRANSACTION_ACCESS
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?;
        operation()
    }

    pub fn write<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _access = TRANSACTION_ACCESS
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?;
        REVISION.fetch_add(1, Ordering::AcqRel);
        let result = operation();
        REVISION.fetch_add(1, Ordering::AcqRel);
        result
    }

    pub fn refresh<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _access = REFRESH_ACCESS
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?;
        operation()
    }

    pub fn stable_revision() -> Result<u64, String> {
        let _access = TRANSACTION_ACCESS
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?;
        Ok(Self::revision())
    }

    pub fn revision() -> u64 {
        REVISION.load(Ordering::Acquire)
    }

    pub fn with_revision<T>(
        revision: u64,
        operation: impl FnOnce() -> T,
    ) -> Result<Option<T>, String> {
        let _access = match TRANSACTION_ACCESS.get_or_init(Default::default).try_lock() {
            Ok(access) => access,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(error)) => return Err(error.to_string()),
        };
        if Self::revision() != revision || !revision.is_multiple_of(2) {
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
        let revision = McpCredentialAccess::stable_revision().unwrap();
        assert_eq!(
            McpCredentialAccess::with_revision(revision, || "current").unwrap(),
            Some("current")
        );

        McpCredentialAccess::write(|| Ok(())).unwrap();

        assert_eq!(
            McpCredentialAccess::with_revision(revision, || "stale").unwrap(),
            None
        );

        let revision = McpCredentialAccess::stable_revision().unwrap();
        let result: Result<(), String> =
            McpCredentialAccess::write(|| Err("possibly changed".to_string()));
        assert!(result.is_err());
        assert_eq!(
            McpCredentialAccess::with_revision(revision, || "current").unwrap(),
            None
        );
    }

    #[test]
    fn launch_validation_does_not_wait_for_writes() {
        let _test = TEST_ACCESS.lock().unwrap();
        let revision = McpCredentialAccess::stable_revision().unwrap();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            McpCredentialAccess::write(|| {
                started_tx.send(()).unwrap();
                finish_rx.recv().unwrap();
                Ok(())
            })
            .unwrap();
        });
        started_rx.recv().unwrap();

        assert_eq!(
            McpCredentialAccess::with_revision(revision, || "stale").unwrap(),
            None
        );

        finish_tx.send(()).unwrap();
        writer.join().unwrap();
    }
}
