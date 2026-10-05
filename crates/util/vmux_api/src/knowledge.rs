#[vmux_api::contract(Default, Eq)]
pub struct KnowledgeReference {
    pub title: String,
    pub path: String,
    pub line: u32,
    pub preview: String,
    pub unlinked: bool,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub enum KnowledgePropertyKind {
    #[default]
    Text,
    Number,
    Checkbox,
    Date,
    List,
    Link,
    Tags,
}

#[vmux_api::contract(Default, Eq)]
pub struct KnowledgeProperty {
    pub key: String,
    pub kind: KnowledgePropertyKind,
    pub values: Vec<String>,
}
