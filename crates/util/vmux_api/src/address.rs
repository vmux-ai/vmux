#[crate::contract(Default)]
pub struct PageAddress {
    pub owner: u64,
    pub revision: u64,
    pub url: String,
    pub current: AddressOption,
    pub views: Vec<AddressOption>,
    pub contextual: Vec<AddressOption>,
    pub fixed: Vec<AddressCrumb>,
    pub paths: Vec<AddressCrumb>,
    pub symbols: Vec<AddressCrumb>,
    pub checkout: Option<AddressOption>,
    pub search_active: bool,
    pub search_available: bool,
    pub controls: Vec<AddressControl>,
    pub toolbar_id: u64,
}

#[crate::contract(Default)]
pub struct AddressControl {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub value: String,
    pub disabled: bool,
    pub active: bool,
    pub primary: bool,
    pub show_label: bool,
    pub danger: bool,
    pub kind: AddressControlKind,
    pub keys: Vec<String>,
    pub options: Vec<AddressControlChoice>,
}

#[crate::contract(Default)]
pub struct AddressControlChoice {
    pub value: String,
    pub label: String,
    pub icon: String,
}

#[crate::contract(Default, Copy, Eq)]
pub enum AddressControlKind {
    #[default]
    Button,
    Search,
    Badge,
    Menu,
}

#[crate::contract(Default)]
pub struct AddressCrumb {
    pub entry: AddressOption,
    pub options: Vec<AddressOption>,
    pub tabs: Vec<AddressOption>,
    pub selected_tab: Option<AddressIntent>,
    pub on_open: Option<AddressIntent>,
    pub on_more: Option<AddressIntent>,
    pub pending: bool,
    pub active: bool,
}

#[crate::contract(Default)]
pub struct AddressOption {
    pub label: String,
    pub label_key: String,
    pub title: String,
    pub identity: String,
    pub icon: Option<crate::BuiltinIcon>,
    pub icon_url: String,
    pub path: String,
    pub is_dir: bool,
    pub intent: Option<AddressIntent>,
}

#[crate::contract]
pub enum AddressIntent {
    Navigate {
        url: String,
    },
    Session {
        id: String,
    },
    SessionLocation {
        value: String,
    },
    Section {
        id: String,
    },
    GitBranch {
        reference: String,
    },
    GitCommit {
        commit: String,
    },
    GitCatalogNext {
        catalog: crate::git::GitCatalog,
    },
    File {
        path: String,
    },
    Preview {
        path: String,
        repository: String,
        checkout: String,
    },
    Branches,
    Siblings {
        path: String,
    },
    Goto {
        line: u32,
    },
    Search,
    Checkout {
        repository: String,
        branch: String,
        checkout: String,
    },
    Control {
        id: String,
        value: String,
    },
    ControlKey {
        id: String,
        key: String,
    },
}

#[crate::ui_event]
pub struct PageAddressRequest {
    pub owner: u64,
    pub revision: u64,
    pub url: String,
    pub intent: AddressIntent,
}

#[crate::ui_event]
pub struct PageAddressPublish {
    pub address: PageAddress,
}

#[crate::ui_event]
pub struct PageToolbarPublish {
    pub id: u64,
    pub controls: Vec<AddressControl>,
}

#[crate::contract]
pub struct AddressEffect {
    pub revision: u64,
    pub owner: u64,
    pub url: String,
    pub address_revision: u64,
    pub intent: AddressIntent,
}

impl AddressEffect {
    pub fn matches(&self, address: &PageAddress) -> bool {
        self.owner == address.owner
            && self.url == address.url
            && self.address_revision == address.revision
    }
}

impl PageAddress {
    pub fn document_url(&self) -> &str {
        self.url.split('#').next().unwrap_or_default()
    }

    pub fn accepts(&self, request: &PageAddressRequest) -> bool {
        self.owner == request.owner && self.revision == request.revision && self.url == request.url
    }
}

#[crate::ui_state_patch(Default)]
pub struct PageAddressUiStatePatch {
    pub address: Option<PageAddress>,
    pub effect: Option<AddressEffect>,
}

#[crate::ui_state(Default, patch = PageAddressUiStatePatch, version = 3)]
pub struct PageAddressUiState {
    pub address: Option<PageAddress>,
    pub effect: Option<AddressEffect>,
}

impl crate::UiStateProjection<PageAddressUiStatePatch> for PageAddressUiState {
    fn apply(&mut self, patch: PageAddressUiStatePatch) {
        if let Some(address) = patch.address {
            if self
                .effect
                .as_ref()
                .is_some_and(|effect| !effect.matches(&address))
            {
                self.effect = None;
            }
            self.address = Some(address);
        }
        if let Some(effect) = patch.effect {
            self.effect = Some(effect);
        }
    }

    fn retain(&mut self) {
        self.effect = None;
    }
}

impl AddressOption {
    pub fn file(label: String, path: String, is_dir: bool) -> Self {
        Self {
            label,
            title: path.clone(),
            path: path.clone(),
            is_dir,
            intent: Some(AddressIntent::File { path }),
            ..Default::default()
        }
    }

    pub fn identity(mut self, identity: String) -> Self {
        self.identity = identity;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UiState, UiStateProjection};

    #[test]
    fn navigation_effects_are_not_replayed_after_reload_or_address_replacement() {
        let address = PageAddress {
            owner: 7,
            revision: 3,
            url: "vmux://settings/".into(),
            ..Default::default()
        };
        let effect = AddressEffect {
            revision: 1,
            owner: address.owner,
            url: address.url.clone(),
            address_revision: address.revision,
            intent: AddressIntent::Section {
                id: "appearance".into(),
            },
        };
        let mut state = PageAddressUiState::from_updates(
            None,
            vec![PageAddressUiStatePatch {
                address: Some(address.clone()),
                effect: Some(effect.clone()),
            }],
        );
        assert!(state.effect.as_ref().unwrap().matches(&address));
        assert!(state.retained().unwrap().effect.is_none());
        let mut replacement = address;
        replacement.url = "vmux://settings/#general".into();
        assert!(!effect.matches(&replacement));
        state.apply(PageAddressUiStatePatch {
            address: Some(replacement),
            ..Default::default()
        });
        assert!(state.effect.is_none());
    }
}
