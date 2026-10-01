use crate::state::ToolProvider;
use vmux_api::VmuxRoute;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ToolRoute {
    #[default]
    Acp,
    Lsp,
    Homebrew,
    Npm,
    Mcp,
    Dotfiles,
}

impl ToolRoute {
    pub(crate) fn parse(url: &str) -> Option<Self> {
        let root = VmuxRoute::parse(crate::ToolPlugin::URL)?;
        let route = VmuxRoute::parse(url)?.canonicalized();
        if route.host() != root.host() || route.query().is_some() || route.fragment().is_some() {
            return None;
        }
        match route.path_segments().next().unwrap_or_default() {
            "acp" => Some(Self::Acp),
            "lsp" => Some(Self::Lsp),
            "homebrew" => Some(Self::Homebrew),
            "npm" => Some(Self::Npm),
            "mcp" => Some(Self::Mcp),
            "dotfiles" => Some(Self::Dotfiles),
            _ => None,
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Acp => "acp",
            Self::Lsp => "lsp",
            Self::Homebrew => "homebrew",
            Self::Npm => "npm",
            Self::Mcp => "mcp",
            Self::Dotfiles => "dotfiles",
        }
    }

    pub(crate) fn url(self) -> String {
        format!("{}{}", crate::ToolPlugin::URL, self.id())
    }

    pub(crate) fn matches(self, provider: ToolProvider) -> bool {
        match self {
            Self::Acp => provider == ToolProvider::Acp,
            Self::Lsp => provider == ToolProvider::Lsp,
            Self::Homebrew => matches!(
                provider,
                ToolProvider::HomebrewFormula | ToolProvider::HomebrewCask
            ),
            Self::Npm => provider == ToolProvider::Npm,
            Self::Mcp => provider == ToolProvider::Mcp,
            Self::Dotfiles => provider == ToolProvider::Dotfiles,
        }
    }
}

impl From<&str> for ToolRoute {
    fn from(url: &str) -> Self {
        Self::parse(url).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_are_derived_from_the_tool_page() {
        assert_eq!(
            ToolRoute::parse(crate::ToolPlugin::URL),
            Some(ToolRoute::Acp)
        );
        assert_eq!(
            ToolRoute::parse(&ToolRoute::Lsp.url()),
            Some(ToolRoute::Lsp)
        );
        assert_eq!(ToolRoute::parse("vmux://settings/"), None);
    }
}
