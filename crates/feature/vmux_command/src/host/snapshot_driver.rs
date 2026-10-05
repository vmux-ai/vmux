use super::snapshot::ClaimedUrl;

impl ClaimedUrl {
    pub(super) fn matches(&self, url: &str) -> bool {
        match (
            vmux_api::VmuxRoute::canonical(&self.0),
            vmux_api::VmuxRoute::canonical(url),
        ) {
            (Some(claimed), Some(candidate)) => claimed == candidate,
            _ => self.0 == url,
        }
    }
}
