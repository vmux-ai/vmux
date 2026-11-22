use crate::state::ToolsNavigateRequest;
use vmux_api::VmuxRoute;
use vmux_api::page_metadata::PageMetadata;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ToolRoute(String);

impl ToolRoute {
    pub(crate) fn named(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub(crate) fn from_metadata(metadata: &PageMetadata) -> Option<Self> {
        Self::parse(&metadata.url)
    }

    pub(crate) fn from_request(request: &ToolsNavigateRequest) -> Option<Self> {
        Self::parse(&request.url)
    }

    pub(crate) fn parse(url: &str) -> Option<Self> {
        let root = VmuxRoute::parse(crate::ToolPlugin::url())?;
        let route = VmuxRoute::parse(url)?.canonicalized();
        if route.host() != root.host() || route.query().is_some() || route.fragment().is_some() {
            return None;
        }
        Some(Self(
            route.path_segments().next().unwrap_or_default().to_string(),
        ))
    }

    pub(crate) fn id(&self) -> &str {
        self.0.as_str()
    }

    pub(crate) fn url(&self) -> String {
        format!("{}{}", crate::ToolPlugin::url(), self.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_are_derived_from_the_tool_page() {
        assert_eq!(
            ToolRoute::parse(crate::ToolPlugin::url()),
            Some(ToolRoute::named("acp"))
        );
        assert_eq!(
            ToolRoute::parse("vmux://tools/lsp").map(|route| route.url()),
            Some("vmux://tools/lsp".to_string())
        );
        assert_eq!(ToolRoute::parse("vmux://settings/"), None);
    }
}
