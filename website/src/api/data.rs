use super::model::{ApiIndex, CrateDoc};

#[cfg(feature = "server")]
fn read(rel: &str) -> Option<String> {
    let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/api/");
    std::fs::read_to_string(format!("{base}{rel}")).ok()
}

#[cfg(all(target_arch = "wasm32", not(feature = "server")))]
async fn fetch(rel: &str) -> Option<String> {
    gloo_net::http::Request::get(&format!("/api/{rel}"))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()
}

#[cfg(feature = "server")]
async fn load(rel: &str) -> Option<String> {
    read(rel)
}

#[cfg(all(target_arch = "wasm32", not(feature = "server")))]
async fn load(rel: &str) -> Option<String> {
    fetch(rel).await
}

#[cfg(all(not(feature = "server"), not(target_arch = "wasm32")))]
async fn load(_rel: &str) -> Option<String> {
    None
}

pub async fn index() -> Option<ApiIndex> {
    let raw = load("index.json").await?;
    serde_json::from_str(&raw).ok()
}

pub async fn crate_doc(name: &str) -> Option<CrateDoc> {
    let raw = load(&format!("{name}.json")).await?;
    serde_json::from_str(&raw).ok()
}
