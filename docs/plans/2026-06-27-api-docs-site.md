# API Docs Site (M0: Pipeline + UI) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate a structured API model from in-code rustdoc comments and render it as a full, vmux-styled API reference under `vmux.ai/docs/api`, without publishing to crates.io/docs.rs.

**Architecture:** A standalone, workspace-excluded `vmux_docs` binary runs nightly rustdoc JSON over the 19 workspace crates, translates the unstable `rustdoc-types` output into our own stable serde schema, and writes committed `docs/api/*.json`. The Dioxus website reads those JSON files and renders crate/module/item pages reusing the existing `markdown::Markdown` renderer, syntect highlighting, and `DocsLayout`. The website deploy never compiles the heavy bevy/CEF crates.

**Tech Stack:** Rust, `rustdoc-json` + `rustdoc-types` (nightly), serde/serde_json, Dioxus 0.7 (fullstack/SSG), pulldown-cmark + syntect, Tailwind, Make, GitHub Actions.

**Reference spec:** `docs/specs/2026-06-27-in-code-api-docs-design.md`

---

## File Structure

**New — generator (`vmux_docs/`, excluded from workspace, own `Cargo.lock`):**
- `vmux_docs/Cargo.toml` — crate manifest + pinned `rustdoc-types`.
- `vmux_docs/src/model.rs` — our stable serde schema (`ApiIndex`, `CrateMeta`, `CrateDoc`, `Module`, `Item`, `Member`, `ItemKind`, `ItemRef`).
- `vmux_docs/src/sig.rs` — `type_to_string` + signature rendering from `rustdoc-types`.
- `vmux_docs/src/translate.rs` — `translate(&rustdoc_types::Crate) -> CrateDoc` (walk, filter, build items).
- `vmux_docs/src/main.rs` — CLI: per-crate `rustdoc-json` build → translate → write `docs/api/*.json`.
- `vmux_docs/tests/fixture/` — tiny crate with known public/private/hidden items (test input).
- `vmux_docs/tests/data/fixture.json` — committed rustdoc JSON of the fixture (test fixture, regenerated when nightly is bumped).
- `vmux_docs/tests/translate_test.rs` — asserts the model produced from `fixture.json`.

**New — committed model output:**
- `docs/api/index.json`, `docs/api/<crate>.json` (one per crate).

**New — website renderer (`website/src/api.rs` + `website/src/api/`):**
- `website/src/api.rs` — module root: re-exports + the `Route` page components (`ApiIndex`, `ApiCrate`, `ApiItem`).
- `website/src/api/model.rs` — serde mirror of the schema (data-only, no rustdoc deps).
- `website/src/api/data.rs` — cfg-split loaders: filesystem read on server (SSG build), `gloo-net` fetch on wasm.
- `website/tests/api_render.rs` — SSR smoke test of `ApiItem` against a fixture model.

**Modified:**
- `Cargo.toml` (root) — add `"vmux_docs"` to `[workspace] exclude`.
- `website/src/main.rs` — add `mod api;`, new routes, extend `static_routes()`, add API sidebar entry.
- `website/src/markdown.rs` — expose `pub fn highlight_code(lang, code) -> String`.
- `website/Cargo.toml` — add `dioxus-ssr` dev-dep for the render test.
- `Makefile` — add `api-docs` target; copy `docs/api` → `website/public/api` in website build steps.
- `.github/workflows/` — new `api-docs-freshness.yml` (path-gated).
- Each of the 19 `crates/*` — seed `//!` + `*Plugin` `///` (Task 12).

---

## Task 0: Pin the nightly toolchain + rustdoc-types

**Files:** none yet (environment setup; record the chosen versions in `vmux_docs/Cargo.toml` and the freshness workflow).

- [ ] **Step 1: Install a recent nightly and read its FORMAT_VERSION**

```bash
rustup toolchain install nightly --profile minimal
rustc +nightly --version            # note the date, e.g. nightly-2026-06-20
```

- [ ] **Step 2: Choose the matching `rustdoc-types` version**

`rustdoc-types` exports a `FORMAT_VERSION` constant per release. Pick the `rustdoc-types` version whose `FORMAT_VERSION` equals the nightly's rustdoc JSON `format_version` (see the rustdoc-types CHANGELOG, which maps crate versions → format version → nightly dates). Record both:

- Nightly: `nightly-YYYY-MM-DD` (the installed one).
- `rustdoc-types = "=X.Y"` (the matching release).

These two are a matched pair pinned everywhere doc-gen runs. Everything downstream depends only on *our* schema, so this pair is the single point that absorbs rustdoc instability.

- [ ] **Step 3: No commit** (this task only records versions used in later tasks).

---

## Task 1: Scaffold `vmux_docs` + stable schema

**Files:**
- Create: `vmux_docs/Cargo.toml`
- Create: `vmux_docs/src/model.rs`
- Create: `vmux_docs/src/main.rs` (temporary stub)
- Modify: `Cargo.toml` (root)
- Test: `vmux_docs/src/model.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Exclude `vmux_docs` from the workspace**

In root `Cargo.toml`, extend the existing exclude list:

```toml
exclude = ["patches/cargo-packager-0.11.8", "website", "vmux_docs"]
```

- [ ] **Step 2: Create `vmux_docs/Cargo.toml`** (use the pinned `rustdoc-types` from Task 0)

```toml
[package]
name = "vmux_docs"
version = "0.1.0"
edition = "2024"
license = "GPL-3.0-or-later"
publish = false

[[bin]]
name = "vmux_docs"
path = "src/main.rs"

[dependencies]
rustdoc-json = "0.9"
rustdoc-types = "=X.Y"          # pinned pair from Task 0
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
clap = { version = "4", features = ["derive"] }
```

- [ ] **Step 3: Write the failing schema round-trip test**

Create `vmux_docs/src/model.rs`:

```rust
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
```

- [ ] **Step 4: Stub `vmux_docs/src/main.rs`** so the crate builds

```rust
mod model;

fn main() {
    println!("vmux_docs");
}
```

- [ ] **Step 5: Run the test (expect PASS)**

Run: `cd vmux_docs && cargo test model::`
Expected: `crate_doc_round_trips ... ok`

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml vmux_docs/Cargo.toml vmux_docs/src/model.rs vmux_docs/src/main.rs vmux_docs/Cargo.lock
git commit -m "feat(docs): scaffold vmux_docs generator + stable API schema"
```

---

## Task 2: Fixture crate + committed rustdoc JSON

**Files:**
- Create: `vmux_docs/tests/fixture/Cargo.toml`
- Create: `vmux_docs/tests/fixture/src/lib.rs`
- Create: `vmux_docs/tests/data/fixture.json` (generated)

- [ ] **Step 1: Create the fixture crate manifest**

`vmux_docs/tests/fixture/Cargo.toml`:

```toml
[package]
name = "fixture"
version = "0.0.1"
edition = "2024"
publish = false

[lib]
path = "src/lib.rs"
```

- [ ] **Step 2: Create fixture source with known public/private/hidden items**

`vmux_docs/tests/fixture/src/lib.rs`:

```rust
//! Fixture crate root docs.

/// A documented struct.
pub struct Widget {
    /// The width field.
    pub width: u32,
}

/// A documented function.
pub fn make(width: u32) -> Widget {
    Widget { width }
}

/// A documented enum.
pub enum Mode {
    /// Fast mode.
    Fast,
    /// Slow mode.
    Slow,
}

struct Hidden;

#[doc(hidden)]
pub struct AlsoHidden;

/// A documented submodule.
pub mod inner {
    //! Inner module docs.

    /// Inner constant.
    pub const ANSWER: u32 = 42;
}
```

- [ ] **Step 3: Generate the committed rustdoc JSON** (one-time; uses the Task 0 nightly)

```bash
cd vmux_docs/tests/fixture
cargo +nightly-YYYY-MM-DD rustdoc --lib -- -Z unstable-options --output-format json
mkdir -p ../data
cp target/doc/fixture.json ../data/fixture.json
cd ../../.. && rm -rf vmux_docs/tests/fixture/target
```

- [ ] **Step 4: Commit**

```bash
git add vmux_docs/tests/fixture/Cargo.toml vmux_docs/tests/fixture/src/lib.rs vmux_docs/tests/data/fixture.json
git commit -m "test(docs): add rustdoc fixture crate + captured JSON"
```

---

## Task 3: Signature rendering (`sig.rs`)

**Files:**
- Create: `vmux_docs/src/sig.rs`
- Modify: `vmux_docs/src/main.rs` (add `mod sig;`)
- Test: inline `#[cfg(test)]` in `sig.rs`

- [ ] **Step 1: Write failing tests for `type_to_string`**

Create `vmux_docs/src/sig.rs`:

```rust
use rustdoc_types::{GenericArg, GenericArgs, Type};

pub fn type_to_string(ty: &Type) -> String {
    match ty {
        Type::ResolvedPath(p) => {
            let mut s = p.path.clone();
            if let Some(args) = p.args.as_deref() {
                s.push_str(&generic_args(args));
            }
            s
        }
        Type::Primitive(p) => p.clone(),
        Type::Generic(g) => g.clone(),
        Type::BorrowedRef { lifetime, is_mutable, type_ } => {
            let mut s = String::from("&");
            if let Some(lt) = lifetime {
                s.push_str(lt);
                s.push(' ');
            }
            if *is_mutable {
                s.push_str("mut ");
            }
            s.push_str(&type_to_string(type_));
            s
        }
        Type::Tuple(items) => {
            let inner: Vec<String> = items.iter().map(type_to_string).collect();
            format!("({})", inner.join(", "))
        }
        Type::Slice(inner) => format!("[{}]", type_to_string(inner)),
        Type::Array { type_, len } => format!("[{}; {}]", type_to_string(type_), len),
        Type::RawPointer { is_mutable, type_ } => {
            let kw = if *is_mutable { "*mut " } else { "*const " };
            format!("{kw}{}", type_to_string(type_))
        }
        Type::QualifiedPath { name, .. } => name.clone(),
        Type::ImplTrait(_) => "impl Trait".into(),
        Type::DynTrait(_) => "dyn Trait".into(),
        Type::Infer => "_".into(),
        _ => "_".into(),
    }
}

fn generic_args(args: &GenericArgs) -> String {
    match args {
        GenericArgs::AngleBracketed { args, .. } if !args.is_empty() => {
            let parts: Vec<String> = args
                .iter()
                .filter_map(|a| match a {
                    GenericArg::Type(t) => Some(type_to_string(t)),
                    GenericArg::Lifetime(lt) => Some(lt.clone()),
                    _ => None,
                })
                .collect();
            if parts.is_empty() {
                String::new()
            } else {
                format!("<{}>", parts.join(", "))
            }
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustdoc_types::Type;

    #[test]
    fn primitive() {
        assert_eq!(type_to_string(&Type::Primitive("u32".into())), "u32");
    }

    #[test]
    fn reference_mut() {
        let t = Type::BorrowedRef {
            lifetime: None,
            is_mutable: true,
            type_: Box::new(Type::Primitive("u8".into())),
        };
        assert_eq!(type_to_string(&t), "&mut u8");
    }

    #[test]
    fn tuple() {
        let t = Type::Tuple(vec![
            Type::Primitive("u8".into()),
            Type::Primitive("bool".into()),
        ]);
        assert_eq!(type_to_string(&t), "(u8, bool)");
    }
}
```

> **Pin note:** the `Type` variant field names above track the `rustdoc-types` version pinned in Task 0. If a field name differs in the pinned release, the compiler error names it exactly — adjust the arm. Do not add fallback guesses.

- [ ] **Step 2: Add `mod sig;` to `main.rs`**

```rust
mod model;
mod sig;

fn main() {
    println!("vmux_docs");
}
```

- [ ] **Step 3: Run the tests (expect PASS)**

Run: `cd vmux_docs && cargo test sig::`
Expected: `primitive`, `reference_mut`, `tuple` all `ok`

- [ ] **Step 4: Commit**

```bash
git add vmux_docs/src/sig.rs vmux_docs/src/main.rs
git commit -m "feat(docs): render rustdoc-types Type to Rust signature strings"
```

---

## Task 4: Translate `rustdoc_types::Crate` → `CrateDoc`

**Files:**
- Create: `vmux_docs/src/translate.rs`
- Create: `vmux_docs/tests/translate_test.rs`
- Modify: `vmux_docs/src/main.rs` (add `mod translate;`, make modules `pub` for the integration test via `lib.rs` — see Step 1)

- [ ] **Step 1: Convert the binary into bin+lib so tests can import modules**

Create `vmux_docs/src/lib.rs`:

```rust
pub mod model;
pub mod sig;
pub mod translate;
```

Update `vmux_docs/src/main.rs`:

```rust
use vmux_docs::translate;

fn main() {
    println!("vmux_docs");
    let _ = translate::translate;
}
```

Add the lib target to `vmux_docs/Cargo.toml` (next to `[[bin]]`):

```toml
[lib]
name = "vmux_docs"
path = "src/lib.rs"
```

- [ ] **Step 2: Write the failing integration test against the fixture JSON**

Create `vmux_docs/tests/translate_test.rs`:

```rust
use vmux_docs::model::ItemKind;
use vmux_docs::translate::translate;

fn load() -> vmux_docs::model::CrateDoc {
    let raw = std::fs::read_to_string("tests/data/fixture.json").unwrap();
    let krate: rustdoc_types::Crate = serde_json::from_str(&raw).unwrap();
    translate(&krate)
}

#[test]
fn captures_public_items_only() {
    let doc = load();
    let names: Vec<&str> = doc.root.items.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"Widget"));
    assert!(names.contains(&"make"));
    assert!(names.contains(&"Mode"));
    assert!(!names.contains(&"Hidden"));      // private
    assert!(!names.contains(&"AlsoHidden"));  // #[doc(hidden)]
}

#[test]
fn carries_docs_and_kind() {
    let doc = load();
    let widget = doc.root.items.iter().find(|i| i.name == "Widget").unwrap();
    assert_eq!(widget.kind, ItemKind::Struct);
    assert_eq!(widget.docs_md, "A documented struct.");
    assert!(widget.members.iter().any(|m| m.name == "width"));
}

#[test]
fn captures_submodule_and_root_docs() {
    let doc = load();
    assert_eq!(doc.root.docs_md, "Fixture crate root docs.");
    let inner = doc.root.submodules.iter().find(|m| m.path.ends_with("inner")).unwrap();
    assert!(inner.items.iter().any(|i| i.name == "ANSWER"));
}
```

Add `serde_json` and `rustdoc-types` are already deps; add `rustdoc-types` to `[dev-dependencies]` is unnecessary (already a normal dep).

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd vmux_docs && cargo test --test translate_test`
Expected: FAIL (`translate` unimplemented / `cannot find function`)

- [ ] **Step 4: Implement `translate.rs`**

```rust
use std::collections::HashMap;

use rustdoc_types::{Crate, Id, Item, ItemEnum};

use crate::model::{CrateDoc, ItemKind, Member, Module};
use crate::sig;

pub fn translate(krate: &Crate) -> CrateDoc {
    let root_item = &krate.index[&krate.root];
    let name = root_item.name.clone().unwrap_or_default();
    let version = krate.crate_version.clone().unwrap_or_default();
    let root = module(krate, &krate.root, &name);
    CrateDoc { name, version, root }
}

fn is_visible(item: &Item) -> bool {
    let hidden = item.attrs.iter().any(|a| a.contains("doc(hidden)"));
    matches!(item.visibility, rustdoc_types::Visibility::Public) && !hidden
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

fn struct_fields(krate: &Crate, s: &rustdoc_types::Struct) -> Vec<Member> {
    let ids = match &s.kind {
        rustdoc_types::StructKind::Plain { fields, .. } => fields.clone(),
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

fn enum_variants(krate: &Crate, e: &rustdoc_types::Enum) -> Vec<Member> {
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

fn links_of(krate: &Crate, item: &Item) -> Vec<crate::model::ItemRef> {
    let mut refs: Vec<crate::model::ItemRef> = item
        .links
        .iter()
        .map(|(text, id): (&String, &Id)| {
            let path = krate.paths.get(id).map(|s| s.path.join("::"));
            crate::model::ItemRef {
                text: text.clone(),
                path,
            }
        })
        .collect();
    refs.sort_by(|a, b| a.text.cmp(&b.text));
    let _ = HashMap::<(), ()>::new();
    refs
}
```

Add to `vmux_docs/src/sig.rs` a `function_signature` helper:

```rust
pub fn function_signature(name: &str, f: &rustdoc_types::Function) -> String {
    let inputs: Vec<String> = f
        .sig
        .inputs
        .iter()
        .map(|(arg, ty)| format!("{arg}: {}", type_to_string(ty)))
        .collect();
    let ret = match &f.sig.output {
        Some(t) => format!(" -> {}", type_to_string(t)),
        None => String::new(),
    };
    format!("pub fn {name}({}){ret}", inputs.join(", "))
}
```

> **Pin note:** `Function.sig` / `FunctionSignature { inputs, output }`, `Struct.kind`, `StructKind::Plain { fields }`, `item.attrs`, `item.visibility`, `Crate.paths[id].path` track the pinned `rustdoc-types`. Adjust field names to the pinned release if the compiler flags them; keep the logic identical.

- [ ] **Step 5: Add `mod translate;`** is already done via `lib.rs` (Step 1). Run the tests:

Run: `cd vmux_docs && cargo test`
Expected: model, sig, and all three `translate_test` cases PASS.

- [ ] **Step 6: Commit**

```bash
git add vmux_docs/src/lib.rs vmux_docs/src/main.rs vmux_docs/src/translate.rs vmux_docs/src/sig.rs vmux_docs/tests/translate_test.rs vmux_docs/Cargo.toml
git commit -m "feat(docs): translate rustdoc JSON into the stable API model"
```

---

## Task 5: Generator CLI driver

**Files:**
- Modify: `vmux_docs/src/main.rs`

> This task wires real rustdoc-json builds. It is **not** unit-tested (it compiles bevy/CEF). It is exercised in the final integration pass (Task 13).

- [ ] **Step 1: Implement the CLI**

```rust
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use vmux_docs::model::{ApiIndex, CrateMeta};
use vmux_docs::translate::translate;

const NIGHTLY: &str = "nightly-YYYY-MM-DD"; // pinned pair (Task 0)

const CRATES: &[&str] = &[
    "vmux_core", "vmux_browser", "vmux_terminal", "vmux_agent", "vmux_editor",
    "vmux_layout", "vmux_git", "vmux_space", "vmux_history", "vmux_command",
    "vmux_service", "vmux_setting", "vmux_mcp", "vmux_team", "vmux_ui",
    "vmux_server", "vmux_desktop", "vmux_cli", "vmux_macro",
];

#[derive(Parser)]
struct Args {
    /// Output directory for the committed model (e.g. ../docs/api).
    #[arg(long, default_value = "../docs/api")]
    out: PathBuf,
    /// Optional subset of crate names; defaults to all.
    #[arg(long)]
    only: Vec<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    std::fs::create_dir_all(&args.out)?;

    let wanted: Vec<&str> = if args.only.is_empty() {
        CRATES.to_vec()
    } else {
        CRATES.iter().copied().filter(|c| args.only.iter().any(|o| o == c)).collect()
    };

    let mut metas = Vec::new();
    for name in wanted {
        eprintln!("doc: {name}");
        let manifest = format!("crates/{name}/Cargo.toml");
        let json_path = rustdoc_json::Builder::default()
            .toolchain(NIGHTLY)
            .manifest_path(&manifest)
            .document_private_items(false)
            .build()?;
        let raw = std::fs::read_to_string(&json_path)?;
        let krate: rustdoc_types::Crate = serde_json::from_str(&raw)?;
        let doc = translate(&krate);
        metas.push(CrateMeta {
            name: doc.name.clone(),
            version: doc.version.clone(),
            blurb_md: first_paragraph(&doc.root.docs_md),
        });
        let out = args.out.join(format!("{name}.json"));
        std::fs::write(out, serde_json::to_string_pretty(&doc)?)?;
    }

    let index = ApiIndex {
        generated_with: NIGHTLY.to_string(),
        crates: metas,
    };
    std::fs::write(args.out.join("index.json"), serde_json::to_string_pretty(&index)?)?;
    Ok(())
}

fn first_paragraph(md: &str) -> String {
    md.split("\n\n").next().unwrap_or("").trim().to_string()
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd vmux_docs && cargo build`
Expected: builds (no run yet — running triggers the heavy crate builds, deferred to Task 13).

- [ ] **Step 3: Commit**

```bash
git add vmux_docs/src/main.rs
git commit -m "feat(docs): vmux_docs CLI generates committed per-crate API JSON"
```

---

## Task 6: Expose `highlight_code` in the website markdown renderer

**Files:**
- Modify: `website/src/markdown.rs`
- Test: inline `#[cfg(test)]` in `markdown.rs`

- [ ] **Step 1: Rename the private fn to a public one**

In `website/src/markdown.rs`, change the signature:

```rust
pub fn highlight_code(lang: &str, code: &str) -> String {
```

and update its single caller in `Node::CodeBlock` rendering:

```rust
let html = highlight_code(lang, code);
```

- [ ] **Step 2: Add a test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_rust_returns_markup() {
        let html = highlight_code("rust", "pub fn x() {}");
        assert!(html.contains("span"));
        assert!(!html.is_empty());
    }
}
```

- [ ] **Step 3: Run the test**

Run: `cd website && cargo test markdown::`
Expected: `highlight_rust_returns_markup ... ok`

- [ ] **Step 4: Commit**

```bash
git add website/src/markdown.rs
git commit -m "feat(website): expose highlight_code for API signature rendering"
```

---

## Task 7: Website schema mirror + data loaders

**Files:**
- Create: `website/src/api.rs`
- Create: `website/src/api/model.rs`
- Create: `website/src/api/data.rs`
- Modify: `website/src/main.rs` (add `mod api;`)
- Test: inline `#[cfg(test)]` in `api/model.rs`

- [ ] **Step 1: Mirror the schema (data-only, no rustdoc deps)**

Create `website/src/api/model.rs` with the **same** structs/enums as `vmux_docs/src/model.rs` (`ApiIndex`, `CrateMeta`, `CrateDoc`, `Module`, `Item`, `Member`, `ItemRef`, `ItemKind`) — copy them verbatim, deriving `Serialize, Deserialize, Clone, PartialEq, Debug`. Add a round-trip test identical in spirit to Task 1 Step 3 (`crate_doc_round_trips`).

- [ ] **Step 2: Write cfg-split loaders**

Create `website/src/api/data.rs`:

```rust
use super::model::{ApiIndex, CrateDoc};

#[cfg(feature = "server")]
fn read(rel: &str) -> Option<String> {
    let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/api/");
    std::fs::read_to_string(format!("{base}{rel}")).ok()
}

#[cfg(all(target_arch = "wasm32", not(feature = "server")))]
async fn fetch(rel: &str) -> Option<String> {
    gloo_net::http::Request::get(&format!("/api/{rel}"))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()
}

pub async fn index() -> Option<ApiIndex> {
    let raw = load("index.json").await?;
    serde_json::from_str(&raw).ok()
}

pub async fn crate_doc(name: &str) -> Option<CrateDoc> {
    let raw = load(&format!("{name}.json")).await?;
    serde_json::from_str(&raw).ok()
}

#[cfg(feature = "server")]
async fn load(rel: &str) -> Option<String> {
    read(rel)
}

#[cfg(all(target_arch = "wasm32", not(feature = "server")))]
async fn load(rel: &str) -> Option<String> {
    fetch(rel).await
}

#[cfg(all(not(feature = "server"), not(target_arch = "wasm32")))]
async fn load(_rel: &str) -> Option<String> {
    None
}
```

- [ ] **Step 3: Create `website/src/api.rs` module root (components added in Task 8)**

```rust
pub mod data;
pub mod model;
```

- [ ] **Step 4: Register the module** — add `mod api;` to `website/src/main.rs` (top, with the other `mod` lines). Add `serde_json` use where needed (it's a transitive dep via dioxus; if not resolvable, add `serde_json = "1"` to `website/Cargo.toml`).

- [ ] **Step 5: Run the test**

Run: `cd website && cargo test api::`
Expected: round-trip test PASS.

- [ ] **Step 6: Commit**

```bash
git add website/src/api.rs website/src/api/model.rs website/src/api/data.rs website/src/main.rs website/Cargo.toml
git commit -m "feat(website): API model mirror + cfg-split JSON loaders"
```

---

## Task 8: API page components + routes + sidebar

**Files:**
- Modify: `website/src/api.rs` (add components)
- Modify: `website/src/main.rs` (routes, `static_routes`, sidebar entry)
- Create: `website/tests/api_render.rs`
- Modify: `website/Cargo.toml` (add `dioxus-ssr` dev-dep)

- [ ] **Step 1: Add page components to `website/src/api.rs`**

Append:

```rust
use dioxus::prelude::*;

use crate::markdown::{highlight_code, Markdown};
use model::{CrateDoc, Item, Module};

#[component]
pub fn ApiIndex() -> Element {
    let idx = use_resource(|| async { data::index().await });
    rsx! {
        h1 { class: "scroll-mt-6 text-3xl sm:text-4xl font-bold tracking-tight mt-4 mb-6", "API Reference" }
        match &*idx.read_unchecked() {
            Some(Some(i)) => rsx! {
                div { class: "grid gap-3 sm:grid-cols-2",
                    for c in i.crates.iter() {
                        Link {
                            class: "block rounded-lg border border-border px-4 py-3 no-underline transition-colors hover:border-accent",
                            to: format!("/docs/api/{}", c.name),
                            div { class: "font-mono text-sm font-medium text-accent", "{c.name}" }
                            div { class: "text-sm text-text-muted", "{c.blurb_md}" }
                        }
                    }
                }
            },
            _ => rsx! { p { class: "text-text-muted", "Loading…" } },
        }
    }
}

#[component]
pub fn ApiCrate(crate_name: String) -> Element {
    let name = crate_name.clone();
    let doc = use_resource(move || {
        let name = name.clone();
        async move { data::crate_doc(&name).await }
    });
    rsx! {
        match &*doc.read_unchecked() {
            Some(Some(d)) => rsx! { {render_module(d, &d.root, true)} },
            _ => rsx! { p { class: "text-text-muted", "No crate \"{crate_name}\"." } },
        }
    }
}

#[component]
pub fn ApiItem(crate_name: String, path: Vec<String>) -> Element {
    let name = crate_name.clone();
    let doc = use_resource(move || {
        let name = name.clone();
        async move { data::crate_doc(&name).await }
    });
    let target = path.join("::");
    rsx! {
        match &*doc.read_unchecked() {
            Some(Some(d)) => {
                match find_item(&d.root, &target) {
                    Some(it) => rsx! { {render_item(it)} },
                    None => rsx! { p { class: "text-text-muted", "No item \"{target}\"." } },
                }
            }
            _ => rsx! { p { class: "text-text-muted", "Loading…" } },
        }
    }
}

fn render_module(doc: &CrateDoc, m: &Module, is_root: bool) -> Element {
    let title = if is_root { doc.name.clone() } else { m.path.clone() };
    rsx! {
        h1 { class: "scroll-mt-6 text-3xl font-bold tracking-tight mt-4 mb-3 font-mono", "{title}" }
        Markdown { content: m.docs_md.clone() }
        if !m.submodules.is_empty() {
            h2 { class: "scroll-mt-6 text-2xl font-semibold mt-10 mb-3 pb-2 border-b border-border", "Modules" }
            ul { class: "list-disc pl-6 my-4 space-y-1.5",
                for sm in m.submodules.iter() {
                    li {
                        Link {
                            class: "text-accent underline underline-offset-2",
                            to: format!("/docs/api/{}/{}", doc.name, sm.path.trim_start_matches(&format!("{}::", doc.name)).replace("::", "/")),
                            "{sm.path}"
                        }
                    }
                }
            }
        }
        if !m.items.is_empty() {
            h2 { class: "scroll-mt-6 text-2xl font-semibold mt-10 mb-3 pb-2 border-b border-border", "Items" }
            ul { class: "list-disc pl-6 my-4 space-y-1.5",
                for it in m.items.iter() {
                    li {
                        Link {
                            class: "text-accent underline underline-offset-2 font-mono text-sm",
                            to: format!("/docs/api/{}/{}", doc.name, it.path.trim_start_matches(&format!("{}::", doc.name)).replace("::", "/")),
                            "{it.name}"
                        }
                        span { class: "text-text-muted text-sm", " — {first_line(&it.docs_md)}" }
                    }
                }
            }
        }
    }
}

fn render_item(it: &Item) -> Element {
    let html = highlight_code("rust", &it.signature);
    rsx! {
        h1 { class: "scroll-mt-6 text-3xl font-bold tracking-tight mt-4 mb-3 font-mono", "{it.name}" }
        pre { class: "bg-code-bg border border-border rounded-lg p-4 my-5 overflow-x-auto",
            code { class: "font-mono text-sm leading-relaxed", dangerous_inner_html: "{html}" }
        }
        Markdown { content: it.docs_md.clone() }
        if !it.members.is_empty() {
            h2 { class: "scroll-mt-6 text-2xl font-semibold mt-10 mb-3 pb-2 border-b border-border", "Members" }
            for mem in it.members.iter() {
                div { class: "my-4",
                    code { class: "font-mono text-[0.85em] bg-code-bg text-accent rounded-md border border-border px-1.5 py-0.5", "{mem.signature}" }
                    Markdown { content: mem.docs_md.clone() }
                }
            }
        }
    }
}

fn find_item<'a>(m: &'a Module, target: &str) -> Option<&'a Item> {
    if let Some(it) = m.items.iter().find(|i| {
        i.path.split("::").skip(1).collect::<Vec<_>>().join("::") == target
    }) {
        return Some(it);
    }
    for sm in &m.submodules {
        if let Some(it) = find_item(sm, target) {
            return Some(it);
        }
    }
    None
}

fn first_line(md: &str) -> String {
    md.lines().next().unwrap_or("").to_string()
}
```

- [ ] **Step 2: Wire routes in `website/src/main.rs`**

Extend the `Route` enum inside the `#[layout(DocsLayout)]` block:

```rust
        #[route("/docs/api")]
        ApiIndex {},
        #[route("/docs/api/:crate_name")]
        ApiCrate { crate_name: String },
        #[route("/docs/api/:crate_name/:..path")]
        ApiItem { crate_name: String, path: Vec<String> },
```

Add re-exports so the route components resolve:

```rust
use api::{ApiCrate, ApiIndex, ApiItem};
```

- [ ] **Step 3: Extend `static_routes()`** to prerender crate pages (item pages render client-side)

```rust
#[server(endpoint = "static_routes", output = server_fn::codec::Json)]
async fn static_routes() -> Result<Vec<String>, ServerFnError> {
    let mut routes = vec!["/".to_string(), "/_home".to_string(), "/docs".to_string()];
    routes.extend(docs::DOCS.iter().map(|d| format!("/docs/{}", d.slug)));
    routes.push("/docs/api".to_string());
    if let Some(idx) = api::data::index().await {
        routes.extend(idx.crates.iter().map(|c| format!("/docs/api/{}", c.name)));
    }
    Ok(routes)
}
```

- [ ] **Step 4: Add an "API Reference" sidebar entry.** In `sidebar()` in `main.rs`, after the doc groups loop, add a static link:

```rust
        div { class: "mb-4",
            div { class: "px-3 mb-1 text-xs uppercase tracking-wide text-text-muted", "Reference" }
            Link {
                class: "block px-3 py-1.5 rounded-md text-sm text-text no-underline hover:bg-surface",
                active_class: "bg-surface text-accent",
                to: Route::ApiIndex {},
                "API Reference"
            }
        }
```

- [ ] **Step 5: Add the SSR test dep + fixture model + smoke test**

Add to `website/Cargo.toml`:

```toml
[dev-dependencies]
dioxus-ssr = "=0.7.4"
```

Create `website/tests/api_render.rs`:

```rust
use dioxus::prelude::*;
use vmux_website::api::model::*;

fn sample() -> Item {
    Item {
        kind: ItemKind::Function,
        name: "make".into(),
        path: "fixture::make".into(),
        signature: "pub fn make(width: u32) -> Widget".into(),
        docs_md: "A documented function.".into(),
        members: vec![],
        links: vec![],
    }
}

#[test]
fn item_renders_signature_and_docs() {
    let it = sample();
    let mut dom = VirtualDom::new_with_props(vmux_website::api::RenderItemProbe, RenderItemProbeProps { item: it });
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("make"));
    assert!(html.contains("documented function"));
}
```

To support the test, expose a probe component in `website/src/api.rs`:

```rust
#[component]
pub fn RenderItemProbe(item: model::Item) -> Element {
    render_item(&item)
}
```

and make the crate testable as a lib: add to `website/Cargo.toml`:

```toml
[lib]
name = "vmux_website"
path = "src/lib.rs"
```

create `website/src/lib.rs` re-exporting the modules used by tests:

```rust
pub mod api;
pub mod hooks;
pub mod markdown;
```

(keep `src/main.rs` as the binary; it can `use vmux_website::...` or keep its own `mod` lines — ensure no duplicate module definitions: switch `main.rs` to `use vmux_website::{api, markdown, hooks, ...};` and `mod docs; mod landing;` for the binary-only modules, OR move all modules into the lib and have `main.rs` call `vmux_website::launch`). Choose the lib-centric layout: move `docs`, `landing` into the lib too and have `main.rs` be a thin launcher.

- [ ] **Step 6: Run the tests**

Run: `cd website && cargo test`
Expected: `item_renders_signature_and_docs` + earlier website tests PASS.

- [ ] **Step 7: Commit**

```bash
git add website/src/api.rs website/src/lib.rs website/src/main.rs website/Cargo.toml website/tests/api_render.rs
git commit -m "feat(website): API reference pages, routes, sidebar + SSR test"
```

---

## Task 9: Makefile target + website static copy

**Files:**
- Modify: `Makefile`

- [ ] **Step 1: Add the `api-docs` generation target**

```make
api-docs: ## regenerate the committed API model from rustdoc
	cd vmux_docs && cargo run --release -- --out ../docs/api
```

Add `api-docs` to the `.PHONY` line.

- [ ] **Step 2: Copy the model into the website build**

In `build-website-css` (the dependency of both `website` and `build-website-release`), append a copy so dev + release both serve the JSON:

```make
build-website-css:
	cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify
	mkdir -p website/public/api && cp -f docs/api/*.json website/public/api/ 2>/dev/null || true
```

- [ ] **Step 3: Commit**

```bash
git add Makefile
git commit -m "build(docs): make api-docs target + copy model into website public"
```

---

## Task 10: CI freshness check

**Files:**
- Create: `.github/workflows/api-docs-freshness.yml`

- [ ] **Step 1: Add the path-gated workflow**

```yaml
name: api-docs-freshness
on:
  pull_request:
    paths:
      - "crates/**"
      - "vmux_docs/**"
      - "docs/api/**"
jobs:
  freshness:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install pinned nightly
        run: rustup toolchain install nightly-YYYY-MM-DD --profile minimal
      - uses: Swatinem/rust-cache@v2
      - name: Regenerate model
        run: make api-docs
      - name: Fail if committed model is stale
        run: git diff --exit-code docs/api/
```

> Linux CI must satisfy the same CEF/system deps the normal build job uses; mirror that job's `apt`/setup steps here (copy from the existing `ci.yml` build job). This job is heavy by nature — it compiles the crates — which is why it is path-gated and isolated from the website-deploy workflow.

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/api-docs-freshness.yml
git commit -m "ci(docs): fail PRs when the committed API model is stale"
```

---

## Task 11: Seed docs — crate-level `//!` + each `*Plugin` `///`

**Files:** each crate root (`crates/<crate>/src/lib.rs`) and the file defining its `*Plugin` struct.

> Content task. For **every** crate, add a one-paragraph `//!` at the crate root and a one-line `///` on its plugin struct. Worked example below; repeat per crate by reading that crate's actual responsibility. Do **not** invent behavior — summarize what the plugin's `build()` actually wires up.

- [ ] **Step 1: Worked example — `vmux_history`**

`crates/vmux_history/src/lib.rs` (top):

```rust
//! History: records visited pages and serves history queries.
//!
//! Spawns visit entries, broadcasts history-changed notifications, prunes old
//! entries on a timer, and answers open/query intents over typed messages.
```

`crates/vmux_history/src/plugin.rs` (above `pub struct HistoryPlugin;`):

```rust
/// Wires the history domain into the app: visit spawning, change broadcasts,
/// timed pruning, and history open/query observers.
```

- [ ] **Step 2: Repeat for the remaining crates** (checkbox each):
  - [ ] vmux_core  - [ ] vmux_browser  - [ ] vmux_terminal  - [ ] vmux_agent
  - [ ] vmux_editor  - [ ] vmux_layout  - [ ] vmux_git  - [ ] vmux_space
  - [ ] vmux_command  - [ ] vmux_service  - [ ] vmux_setting  - [ ] vmux_mcp
  - [ ] vmux_team  - [ ] vmux_ui  - [ ] vmux_server  - [ ] vmux_desktop
  - [ ] vmux_cli  - [ ] vmux_macro

- [ ] **Step 3: Verify each crate still builds its docs locally is deferred to Task 13.** Commit the seed docs:

```bash
git add crates/*/src/lib.rs crates/*/src/plugin.rs
git commit -m "docs: seed crate-level and plugin doc comments for all crates"
```

---

## Task 12: First real generation + commit the model

**Files:** `docs/api/*.json` (generated)

> Heavy: this compiles all 19 crates (bevy/CEF). Run once with a warm target dir.

- [ ] **Step 1: Generate the full model**

```bash
make api-docs
ls docs/api/        # index.json + 19 crate files
```

- [ ] **Step 2: Sanity-check a file**

```bash
head -40 docs/api/vmux_history.json
```

Expected: valid JSON with `name`, `version`, `root.items[...]`.

- [ ] **Step 3: Commit the generated model**

```bash
git add docs/api/
git commit -m "docs(api): generate initial committed API model"
```

---

## Task 13: Final integration pass (manual)

> Per project workflow, defer runtime/manual verification to one pass at the end.

- [ ] **Step 1: Full workspace tests**

Run: `cargo test --workspace`
Expected: green (includes the `no_continuous_update_mode` and other existing tests).

- [ ] **Step 2: Generator + website unit tests**

Run: `cd vmux_docs && cargo test && cd ../website && cargo test`
Expected: all green.

- [ ] **Step 3: fmt + clippy**

Run: `cargo fmt --all && cargo clippy --workspace` and `cd vmux_docs && cargo fmt && cargo clippy` and `cd website && cargo fmt && cargo clippy`.
After `cargo fmt`, restore any vendored patch reformatting: `git checkout -- patches/` (fmt reformats vendored crates).
Expected: no warnings; only intended diffs staged.

- [ ] **Step 4: Build + serve the website locally and click through**

Run: `make website`
Visit `/docs/api`, open a crate (e.g. `vmux_history`), open an item. Confirm: vmux styling, signature block highlighted, doc body rendered, sidebar "API Reference" entry, prev/next unaffected on prose docs.

- [ ] **Step 5: Release SSG build smoke**

Run: `make build-website-release`
Confirm `docs/api/*.json` present under the published `public/api/` and `/docs/api/<crate>` pages prerendered.

- [ ] **Step 6: Open the PR**

```bash
git push -u origin feat/api-docs-site
gh pr create --title "feat(docs): in-code API reference at vmux.ai/docs/api" --body "Implements docs/specs/2026-06-27-in-code-api-docs-design.md (M0). Part B per-crate authoring follows."
```

- [ ] **Step 7: Delete this plan file once merged** (per AGENTS.md).

---

## Part B rollout (separate cycles — not this plan)

Each crate, in the spec's B3 importance order, is its own cycle:

1. New worktree/branch per crate (or batch a few).
2. Write `///` across that crate's full public API (first line = summary; bodies are Markdown with ```rust examples and `[Type]` intra-doc links).
3. Enable `#![warn(missing_docs)]` at that crate root once complete.
4. `make api-docs` → commit `docs/api/<crate>.json`.
5. Review the rendered `/docs/api/<crate>` page.

Definition of done per crate: no `missing_docs` warnings, model regenerated + committed, page reviewed.

---

## Self-Review

**Spec coverage:**
- Generator `vmux_docs` (excluded crate, nightly, rustdoc-json/types) → Tasks 1,3,4,5. ✔
- Stable serde model decoupled from rustdoc-types → Tasks 1 (gen) + 7 (website mirror). ✔
- Committed `docs/api/*.json` → Tasks 5,12. ✔
- vmux-styled renderer reusing markdown + syntect + DocsLayout → Tasks 6,8. ✔
- Routes `/docs/api`, `/docs/api/:crate`, item pages; SSG enumerates crates, items client-side → Task 8. ✔
- Bundle strategy (static assets, not include_str!) → Tasks 7 (data.rs), 9 (copy). ✔
- Makefile `api-docs` + light website deploy → Task 9. ✔
- CI freshness check, path-gated → Task 10. ✔
- Seed docs (`//!` + plugin `///`) all 19 → Task 11. ✔
- Testing: fixture-based generator tests, schema round-trip (both sides), renderer SSR smoke, CI staleness → Tasks 2,3,4,7,8,10,13. ✔
- Part B conventions/DoD/order → "Part B rollout" section. ✔

**Placeholder scan:** `nightly-YYYY-MM-DD` and `rustdoc-types = "=X.Y"` are the deliberately-pinned pair from Task 0 (recorded once, substituted everywhere) — a setup action, not an unfilled blank. The two "Pin note" callouts flag where upstream field names must match that pin. No other TBD/TODO.

**Type consistency:** `ItemKind`, `Item{kind,name,path,signature,docs_md,members,links}`, `Member{name,signature,docs_md}`, `ItemRef{text,path}`, `CrateDoc{name,version,root}`, `Module{path,docs_md,items,submodules}` identical across `vmux_docs/src/model.rs` (Task 1) and `website/src/api/model.rs` (Task 7). `highlight_code` defined in Task 6, used in Task 8. `data::index`/`data::crate_doc` defined in Task 7, used in Task 8. Routes `ApiIndex/ApiCrate/ApiItem` defined and wired in Task 8.

**Two impl-time verify points (flagged, not placeholders):**
1. Exact `rustdoc-types` field names vs the pinned nightly (Pin notes in Tasks 3,4) — caught immediately by the fixture round-trip test.
2. Dioxus SSG data-await: Tasks 8 use `use_resource` (item pages client-side). If crate-page prerender needs data at SSR, switch those two to `use_server_future`; verified in Task 13 Step 5.
