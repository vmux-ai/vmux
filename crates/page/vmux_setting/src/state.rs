#[vmux_api::ui_state(Default, url = "vmux://settings/")]
pub struct SettingsUiState {
    pub sections: Vec<SettingsSection>,
    pub fields: Vec<SettingsRenderField>,
    pub items: Vec<SettingsRenderItem>,
}

#[vmux_api::contract]
pub struct SettingsSection {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub search_text: String,
    pub fields: Vec<SettingsRenderFieldId>,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub struct SettingsRenderFieldId(pub u32);

#[vmux_api::contract(Copy, Eq, Default)]
pub struct SettingsRenderItemId(pub u32);

#[vmux_api::contract]
pub struct SettingsRenderField {
    pub path: String,
    pub label: String,
    #[serde(default)]
    pub hint: Option<String>,
    pub kind: SettingsRenderFieldKind,
}

#[vmux_api::contract]
pub enum SettingsRenderFieldKind {
    Toggle {
        value: bool,
    },
    Integer {
        value: u64,
    },
    Number {
        value: f64,
        step: f64,
    },
    Text {
        value: String,
        #[serde(default)]
        placeholder: Option<String>,
    },
    Select {
        value: String,
        options: Vec<SettingsSelectOption>,
    },
    Chord {
        text: String,
    },
    Bindings {
        rows: Vec<SettingsBindingRow>,
    },
    Group {
        fields: Vec<SettingsRenderFieldId>,
    },
    Array {
        items: Vec<SettingsRenderItemId>,
    },
    UpdateCheck {
        button_label: String,
        disabled: bool,
    },
}

#[vmux_api::contract]
pub struct SettingsSelectOption {
    pub value: String,
    pub label: String,
}

#[vmux_api::contract]
pub struct SettingsBindingRow {
    pub command: String,
    pub chord: String,
    #[serde(default)]
    pub edit_path: Option<String>,
}

#[vmux_api::contract]
pub enum SettingsRenderItem {
    Value {
        text: String,
    },
    Object {
        title: String,
        fields: Vec<SettingsRenderFieldId>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_snapshot_round_trips() {
        let state = SettingsUiState {
            sections: vec![SettingsSection {
                id: "general".into(),
                title: "General".into(),
                description: None,
                search_text: "general auto update".into(),
                fields: vec![SettingsRenderFieldId(0)],
            }],
            fields: vec![SettingsRenderField {
                path: "auto_update".into(),
                label: "Auto update".into(),
                hint: None,
                kind: SettingsRenderFieldKind::Toggle { value: true },
            }],
            items: Vec::new(),
        };

        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&state).unwrap();
        let decoded = rkyv::from_bytes::<SettingsUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(decoded, state);
    }
}
