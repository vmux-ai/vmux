#[crate::contract(Default)]
pub struct PageAddress {
    pub owner: u64,
    pub revision: u64,
    pub url: String,
    pub page: String,
    pub contextual: Vec<AddressOption>,
    pub fixed: Vec<AddressCrumb>,
    pub paths: Vec<AddressCrumb>,
    pub symbols: Vec<AddressCrumb>,
    pub checkout: Option<AddressOption>,
    pub search_active: bool,
}

#[crate::contract(Default)]
pub struct AddressCrumb {
    pub entry: AddressOption,
    pub options: Vec<AddressOption>,
    pub on_open: Option<AddressIntent>,
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
    pub path: String,
    pub is_dir: bool,
    pub intent: Option<AddressIntent>,
}

#[crate::contract]
pub enum AddressIntent {
    Navigate {
        url: String,
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
}

#[crate::ui_event]
pub struct PageAddressRequest {
    pub owner: u64,
    pub revision: u64,
    pub url: String,
    pub intent: AddressIntent,
}

impl PageAddress {
    pub fn accepts(&self, request: &PageAddressRequest) -> bool {
        self.owner == request.owner && self.revision == request.revision && self.url == request.url
    }
}

#[crate::ui_state_patch(Default)]
pub struct PageAddressUiStatePatch {
    pub address: Option<PageAddress>,
    pub in_header: Option<bool>,
}

#[crate::ui_state(Default, patch = PageAddressUiStatePatch, version = 1)]
pub struct PageAddressUiState {
    pub address: Option<PageAddress>,
    pub in_header: bool,
}

impl crate::UiStateProjection<PageAddressUiStatePatch> for PageAddressUiState {
    fn apply(&mut self, patch: PageAddressUiStatePatch) {
        if let Some(address) = patch.address {
            self.address = Some(address);
        }
        if let Some(in_header) = patch.in_header {
            self.in_header = in_header;
        }
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
