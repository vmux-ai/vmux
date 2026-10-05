pub struct Platform;

impl Platform {
    pub fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or_default()
    }

    pub async fn sleep(ms: u32) {
        tokio::time::sleep(std::time::Duration::from_millis(u64::from(ms))).await;
    }

    pub async fn copy(text: String) -> bool {
        vmux_clipboard::Clipboard::write(text);
        true
    }

    pub fn random_index(len: usize) -> usize {
        if len == 0 {
            return 0;
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.subsec_nanos() as usize)
            .unwrap_or_default();
        nanos % len
    }
}
