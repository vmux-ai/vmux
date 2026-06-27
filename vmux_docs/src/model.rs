use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiIndex {
    pub generated_with: String,
    pub crates: Vec<CrateMeta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrateMeta {
    pub name: String,
    pub version: String,
    pub blurb_md: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrateDoc {
    pub name: String,
    pub version: String,
    pub root: Module,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub path: String,
    pub docs_md: String,
    pub items: Vec<Item>,
    pub submodules: Vec<Module>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub kind: ItemKind,
    pub name: String,
    pub path: String,
    pub signature: String,
    pub docs_md: String,
    pub members: Vec<Member>,
    pub links: Vec<ItemRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub name: String,
    pub signature: String,
    pub docs_md: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemRef {
    pub text: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Struct,
    Enum,
    Trait,
    Function,
    TypeAlias,
    Constant,
    Macro,
    Module,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_doc_round_trips() {
        let doc = CrateDoc {
            name: "vmux_demo".into(),
            version: "0.0.1".into(),
            root: Module {
                path: "vmux_demo".into(),
                docs_md: "Root docs.".into(),
                items: vec![Item {
                    kind: ItemKind::Struct,
                    name: "Foo".into(),
                    path: "vmux_demo::Foo".into(),
                    signature: "pub struct Foo".into(),
                    docs_md: "A foo.".into(),
                    members: vec![Member {
                        name: "bar".into(),
                        signature: "bar: u32".into(),
                        docs_md: "the bar".into(),
                    }],
                    links: vec![],
                }],
                submodules: vec![],
            },
        };
        let json = serde_json::to_string(&doc).unwrap();
        let back: CrateDoc = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, back);
    }
}
