use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SettingsSchema {
    #[serde(default)]
    pub sections: Vec<SectionSpec>,
    #[serde(default)]
    pub fields: Vec<(String, FieldSpec)>,
}

impl SettingsSchema {
    pub fn field(&self, path: &str) -> Option<&FieldSpec> {
        self.fields.iter().find(|(p, _)| p == path).map(|(_, s)| s)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SectionSpec {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub synthetic_keys: Vec<String>,
    #[serde(default)]
    pub root_path: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FieldSpec {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub widget: Option<WidgetKind>,
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default)]
    pub omit: bool,
    #[serde(default)]
    pub step: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WidgetKind {
    LeaderKbd,
    BindingsList,
}
