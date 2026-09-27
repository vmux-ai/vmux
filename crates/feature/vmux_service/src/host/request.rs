use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, oneshot};

pub(crate) struct PendingRequests<K, V> {
    entries: Arc<Mutex<HashMap<K, oneshot::Sender<V>>>>,
}

impl<K, V> Clone for PendingRequests<K, V> {
    fn clone(&self) -> Self {
        Self {
            entries: Arc::clone(&self.entries),
        }
    }
}

impl<K, V> Default for PendingRequests<K, V> {
    fn default() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<K, V> PendingRequests<K, V>
where
    K: Copy + Eq + Hash,
{
    pub(crate) async fn request(
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

    pub(crate) async fn resolve(&self, id: K, response: V) -> bool {
        let Some(sender) = self.entries.lock().await.remove(&id) else {
            return false;
        };
        sender.send(response).is_ok()
    }
}
