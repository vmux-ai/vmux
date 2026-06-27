use rustdoc_types::{Crate, Enum, Id, Item, ItemEnum, Struct, StructKind, Visibility};

use crate::model::{CrateDoc, ItemKind, ItemRef, Member, Module};
use crate::sig;

pub fn translate(krate: &Crate) -> CrateDoc {
    let root_item = &krate.index[&krate.root];
    let name = root_item.name.clone().unwrap_or_default();
    let version = krate.crate_version.clone().unwrap_or_default();
    let root = module(krate, &krate.root, &name);
    CrateDoc {
        name,
        version,
        root,
    }
}

fn is_visible(item: &Item) -> bool {
    matches!(item.visibility, Visibility::Public)
}

fn docs_of(item: &Item) -> String {
    item.docs.clone().unwrap_or_default()
}

fn module(krate: &Crate, id: &Id, path: &str) -> Module {
    let item = &krate.index[id];
    let ItemEnum::Module(m) = &item.inner else {
        return Module {
            path: path.to_string(),
            docs_md: docs_of(item),
            items: vec![],
            submodules: vec![],
        };
    };
    let mut items = Vec::new();
    let mut submodules = Vec::new();
    for child_id in &m.items {
        let Some(child) = krate.index.get(child_id) else {
            continue;
        };
        if !is_visible(child) {
            continue;
        }
        let Some(child_name) = &child.name else {
            continue;
        };
        let child_path = format!("{path}::{child_name}");
        match &child.inner {
            ItemEnum::Module(_) => submodules.push(module(krate, child_id, &child_path)),
            _ => {
                if let Some(it) = item_of(krate, child, &child_path) {
                    items.push(it);
                }
            }
        }
    }
    Module {
        path: path.to_string(),
        docs_md: docs_of(item),
        items,
        submodules,
    }
}

fn item_of(krate: &Crate, item: &Item, path: &str) -> Option<crate::model::Item> {
    let name = item.name.clone()?;
    let (kind, signature, members) = match &item.inner {
        ItemEnum::Struct(s) => (
            ItemKind::Struct,
            format!("pub struct {name}"),
            struct_fields(krate, s),
        ),
        ItemEnum::Enum(e) => (
            ItemKind::Enum,
            format!("pub enum {name}"),
            enum_variants(krate, e),
        ),
        ItemEnum::Trait(_) => (ItemKind::Trait, format!("pub trait {name}"), vec![]),
        ItemEnum::Function(f) => (
            ItemKind::Function,
            sig::function_signature(&name, f),
            vec![],
        ),
        ItemEnum::TypeAlias(_) => (ItemKind::TypeAlias, format!("pub type {name}"), vec![]),
        ItemEnum::Constant { .. } => (ItemKind::Constant, format!("pub const {name}"), vec![]),
        ItemEnum::Macro(_) => (ItemKind::Macro, format!("macro_rules! {name}"), vec![]),
        _ => return None,
    };
    Some(crate::model::Item {
        kind,
        name,
        path: path.to_string(),
        signature,
        docs_md: docs_of(item),
        members,
        links: links_of(krate, item),
    })
}

fn struct_fields(krate: &Crate, s: &Struct) -> Vec<Member> {
    let ids = match &s.kind {
        StructKind::Plain { fields, .. } => fields.clone(),
        _ => vec![],
    };
    field_members(krate, &ids)
}

fn field_members(krate: &Crate, ids: &[Id]) -> Vec<Member> {
    let mut out = Vec::new();
    for id in ids {
        let Some(item) = krate.index.get(id) else {
            continue;
        };
        if !is_visible(item) {
            continue;
        }
        let Some(name) = item.name.clone() else {
            continue;
        };
        let signature = match &item.inner {
            ItemEnum::StructField(ty) => format!("{name}: {}", sig::type_to_string(ty)),
            _ => name.clone(),
        };
        out.push(Member {
            name,
            signature,
            docs_md: docs_of(item),
        });
    }
    out
}

fn enum_variants(krate: &Crate, e: &Enum) -> Vec<Member> {
    let mut out = Vec::new();
    for id in &e.variants {
        let Some(item) = krate.index.get(id) else {
            continue;
        };
        let Some(name) = item.name.clone() else {
            continue;
        };
        out.push(Member {
            name: name.clone(),
            signature: name,
            docs_md: docs_of(item),
        });
    }
    out
}

fn links_of(krate: &Crate, item: &Item) -> Vec<ItemRef> {
    let mut refs: Vec<ItemRef> = item
        .links
        .iter()
        .map(|(text, id)| ItemRef {
            text: text.clone(),
            path: krate.paths.get(id).map(|s| s.path.join("::")),
        })
        .collect();
    refs.sort_by(|a, b| a.text.cmp(&b.text));
    refs
}
