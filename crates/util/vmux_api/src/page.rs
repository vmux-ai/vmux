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
        crate::RoutePattern::new(self.url, self.owns_subtree)
            .specificity(page_url)
            .map(|(_, specificity)| specificity)
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
        T: crate::UiState,
        crate::UiStateFrame<T, T::Update>: for<'a> rkyv::Serialize<
                rkyv::api::high::HighSerializer<
                    rkyv::util::AlignedVec,
                    rkyv::ser::allocator::ArenaHandle<'a>,
                    rkyv::rancor::Error,
                >,
            >,
    {
        let frame = crate::UiStateFrame::<T, T::Update>::Snapshot(state.clone());
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).ok()?;
        Some(Self {
            id: T::id().to_string(),
            bytes: bytes.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[vmux_api::ui_state(Default, Eq)]
    struct TestState {
        value: u32,
    }

    #[test]
    fn ui_state_emit_uses_the_shared_frame_contract() {
        let emit = UiStateEmit::from_state(&TestState { value: 7 }).unwrap();
        let frame = rkyv::from_bytes::<
            crate::UiStateFrame<TestState, TestState>,
            rkyv::rancor::Error,
        >(&emit.bytes)
        .unwrap();

        assert_eq!(frame, crate::UiStateFrame::Snapshot(TestState { value: 7 }));
    }

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
