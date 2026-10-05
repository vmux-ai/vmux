use bevy_ecs::message::Message;

#[derive(bevy_ecs::component::Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiEventPermissions {
    pub url: &'static str,
    pub owns_subtree: bool,
    pub permissions: &'static [&'static str],
}

impl UiEventPermissions {
    pub fn allows(&self, page_url: &str, permission: &str) -> bool {
        self.answers_for(page_url) && self.permissions.contains(&permission)
    }

    pub fn answers_for(&self, page_url: &str) -> bool {
        self.specificity(page_url).is_some()
    }

    pub fn allows_page<'a>(
        pages: impl IntoIterator<Item = &'a Self>,
        page_url: &str,
        permission: &str,
    ) -> bool {
        let mut specificity = None;
        let mut allowed = false;
        for page in pages {
            let Some(candidate) = page.specificity(page_url) else {
                continue;
            };
            match specificity {
                Some(current) if candidate < current => {}
                Some(current) if candidate == current => {
                    allowed |= page.permissions.contains(&permission);
                }
                _ => {
                    specificity = Some(candidate);
                    allowed = page.permissions.contains(&permission);
                }
            }
        }
        allowed
    }

    fn specificity(&self, page_url: &str) -> Option<usize> {
        if self.url.ends_with("://") {
            return page_url.starts_with(self.url).then_some(self.url.len());
        }
        if let (Some(page), Some(candidate)) = (
            crate::VmuxRoute::parse(self.url),
            crate::VmuxRoute::parse(page_url),
        ) {
            let matches = match self.owns_subtree {
                true => candidate.in_subtree(&page),
                false => candidate.same_page(&page),
            };
            return matches.then_some(self.url.len());
        }
        let (Ok(page), Ok(candidate)) = (url::Url::parse(self.url), url::Url::parse(page_url))
        else {
            return None;
        };
        if page.scheme() != candidate.scheme() || page.host_str() != candidate.host_str() {
            return None;
        }
        let page_path = page.path().trim_end_matches('/');
        let candidate_path = candidate.path().trim_end_matches('/');
        let matches = candidate_path == page_path
            || (self.owns_subtree
                && candidate_path
                    .strip_prefix(page_path)
                    .is_some_and(|suffix| suffix.starts_with('/')));
        matches.then_some(self.url.len())
    }
}

#[derive(Message)]
pub struct UiStateEmit {
    pub id: String,
    pub bytes: Vec<u8>,
}

impl UiStateEmit {
    pub fn from_state<T>(state: &T) -> Option<Self>
    where
        T: crate::UiState
            + for<'a> rkyv::Serialize<
                rkyv::api::high::HighSerializer<
                    rkyv::util::AlignedVec,
                    rkyv::ser::allocator::ArenaHandle<'a>,
                    rkyv::rancor::Error,
                >,
            >,
    {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(state).ok()?;
        Some(Self {
            id: T::id().to_string(),
            bytes: bytes.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_permissions_use_exact_names() {
        let permissions = UiEventPermissions {
            url: "vmux://tools/",
            owns_subtree: true,
            permissions: &["ToolsRefreshRequest"],
        };

        assert!(permissions.allows("vmux://tools/npm", "ToolsRefreshRequest"));
        assert!(!permissions.allows("vmux://tools/npm", "ToolInstallRequest"));
        assert!(!permissions.allows("vmux://layout/", "ToolsRefreshRequest"));
    }

    #[test]
    fn the_most_specific_page_owns_its_permissions() {
        let pages = [
            UiEventPermissions {
                url: "vmux://tools/",
                owns_subtree: true,
                permissions: &["BroadRequest"],
            },
            UiEventPermissions {
                url: "vmux://tools/lsp",
                owns_subtree: false,
                permissions: &["LspRequest"],
            },
        ];

        assert!(UiEventPermissions::allows_page(
            &pages,
            "vmux://tools/lsp",
            "LspRequest"
        ));
        assert!(!UiEventPermissions::allows_page(
            &pages,
            "vmux://tools/lsp",
            "BroadRequest"
        ));
        assert!(UiEventPermissions::allows_page(
            &pages,
            "vmux://tools/npm",
            "BroadRequest"
        ));
    }
}
