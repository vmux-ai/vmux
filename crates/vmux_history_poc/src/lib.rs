pub const HISTORY_EVENT: &str = "history";

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HistoryEvent {
    pub url: String,
    pub history: Vec<String>,
}

#[cfg(target_arch = "wasm32")]
mod app;

#[cfg(target_arch = "wasm32")]
pub use app::App;
