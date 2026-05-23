# OpenCommand Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consolidate the five scattered "open new page" commands into a single `OpenCommand` enum under `BrowserCommand`, with optional URL payload, MCP exposure, and tmux-shortcut backwards compatibility.

**Architecture:** Extend `vmux_macro` derives to accept `Fields::Named` variants plus two new attributes (`expand` for direction fan-out, `chord = ..., variant = ...` for legacy chord rebinding). Restructure `BrowserCommand` into an `OsSubMenuGroup` of four sub-enums (Navigation, Open, View, Bar). Add `OpenCommand` with five variants and route every "open URL" trigger (menu, shortcut, command bar, MCP, terminal, agent, history) through one set of handlers. Migrate the webview IPC (`CommandBarOpenEvent.new_tab`, `CommandBarActionEvent.action`) to a typed `OpenTarget` enum.

**Tech Stack:** Rust 2024, Bevy ECS, `proc_macro2` / `syn` / `quote` for derive macros, `muda` for native menus, `rkyv` for IPC binary serialisation, `bevy_cef` for webview, MCP via `vmux_mcp`.

**Spec:** `docs/specs/2026-05-22-open-command-consolidation-design.md`

---

## File Structure

### Create

- `crates/vmux_command/src/open.rs` — `OpenCommand` enum, `PaneDirection`, `PaneTarget`, `PaneOpenMode`, `OpenTarget`, `Default` impls
- `crates/vmux_command/src/open/handler.rs` — handler systems for the five `OpenCommand` variants, the `resolve_url` helper, and the `Existing → NewSplit` fallback
- `crates/vmux_layout/src/cleanup_open_target.rs` — runtime test fixtures for the IPC migration (tiny module)
- `crates/vmux_macro/src/named_fields.rs` — shared logic for instantiating named-field variants with `Default::default()` (used by all three extended derives)
- `crates/vmux_macro/src/expand.rs` — `expand = "<field>"` attribute parsing + enum-variant enumeration

### Modify

- `crates/vmux_macro/src/lib.rs` — relax `Fields::Unit` check in `OsSubMenu`, `DefaultShortcuts`, `CommandBar`; add new attribute handling (lines ≈ 73, 394, 663, plus per-derive expansion sites)
- `crates/vmux_command/src/command.rs` — restructure `BrowserCommand` to `OsSubMenuGroup`; introduce `BrowserNavigationCommand`, `BrowserOpenWrapper` (or use `OpenCommand` directly), `BrowserViewCommand`, `BrowserBarCommand`; drop `StackCommand::New`, `TabCommand::New`, `PaneCommand::SplitV/SplitH`, `TerminalCommand::New/NewTab`, `BrowserCommand::FocusAddressBar`
- `crates/vmux_command/src/event.rs` — replace `CommandBarOpenEvent.new_tab: bool` with `target: Option<OpenTarget>`; remove `CommandBarActionEvent.action` reliance (event still exists for path-completion etc.; the navigate action becomes a direct `OpenCommand` emit)
- `crates/vmux_command/src/lib.rs` — re-export `open::*`
- `crates/vmux_layout/src/stack.rs:189` (handler), `stack.rs:562` — drop `StackCommand::New` arm; add `OpenCommand::InNewStack` handler call
- `crates/vmux_layout/src/space.rs:181` — drop `TabCommand::New` arm; add `OpenCommand::InNewTab` handler call
- `crates/vmux_layout/src/pane.rs:514-520` — drop `PaneCommand::SplitV/SplitH` arm; add `OpenCommand::InPane` handler call (covers all 4 directions × 2 targets × 2 modes)
- `crates/vmux_layout/src/command_bar/handler.rs` — replace `action: "navigate" | "new_tab"` string dispatch with direct `OpenCommand` emission; carry `target: Option<OpenTarget>` from modal open event
- `crates/vmux_terminal/src/plugin.rs` — `TerminalCommand::New` / `NewTab` callers now emit `OpenCommand::InNewStack` / `InNewTab` with `vmux://terminal/` URL
- `crates/vmux_desktop/src/shortcut.rs` — rebind shortcuts: `super+n` → `InNewStack`, `super+t` → `InNewTab`, `super+l` → `InPlace`, `super+shift+n` → `InNewSpace`, `Super+Shift+H/J/K/L` → `InPane`, `ctrl+\`` → `InNewStack` with terminal URL, `Ctrl+g, %` / `Ctrl+g, "` → `InPane Right/Bottom NewSplit NewStack`
- `crates/vmux_desktop/src/command_bar.rs` — replace `BrowserCommand::FocusAddressBar` / `CommandBarOpenEvent.new_tab` writes with `OpenCommand` emissions
- `crates/vmux_history/src/plugin.rs` — `OpenInNewStack` history-click handler routes through `OpenCommand::InNewStack`
- `crates/vmux_agent/src/plugin.rs` — agent `OpenInNewStack` request routes through `OpenCommand::InNewStack`
- `crates/vmux_page/`* and `crates/vmux_layout/src/command_bar/page.rs` (webview side) — emit typed `OpenTarget` in modal open payload
- `crates/vmux_command/tests/` — extend `from_menu_id` round-trip tests for the new IDs; remove obsolete `stack_new` / `new_tab` / `split_v` entries

### Delete

- (none — all changes are additive or in-place removal)

---

## Task 1: Macro — accept `Fields::Named` in `OsSubMenu`

**Files:**
- Modify: `crates/vmux_macro/src/lib.rs` (the `impl_os_sub_menu` function, around the `Fields::Unit` check at line 73)
- Create: `crates/vmux_macro/src/named_fields.rs`
- Test: `crates/vmux_macro/tests/named_field_variants.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/vmux_macro/tests/named_field_variants.rs
use vmux_macro::OsSubMenu;

#[derive(OsSubMenu, Debug, Clone, PartialEq, Eq, Default)]
enum Sample {
    #[default]
    #[menu(id = "sample_a", label = "Sample A")]
    A { url: Option<String> },
    #[menu(id = "sample_b", label = "Sample B")]
    B { name: Option<String> },
}

#[test]
fn from_menu_id_returns_variant_with_default_fields() {
    let parsed = Sample::from_menu_id("sample_a").expect("sample_a should resolve");
    assert_eq!(parsed, Sample::A { url: None });
}

#[test]
fn from_menu_id_handles_multiple_named_field_variants() {
    let parsed = Sample::from_menu_id("sample_b").expect("sample_b should resolve");
    assert_eq!(parsed, Sample::B { name: None });
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `bash -c "cargo test -p vmux_macro --test named_field_variants 2>&1 | tail -30"`
Expected: compile error from the derive — current code rejects `Fields::Named` at the `Fields::Unit` guard.

- [ ] **Step 3: Add the shared helper**

```rust
// crates/vmux_macro/src/named_fields.rs
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, Ident};

/// Build the constructor expression for a variant with `Fields::Named` where
/// every field receives its `Default::default()` value. Used by OsSubMenu /
/// DefaultShortcuts / CommandBar when binding a menu-id or shortcut to a
/// variant without a payload.
pub fn build_default_named_constructor(
    enum_ident: &Ident,
    variant_ident: &Ident,
    fields: &Fields,
) -> TokenStream {
    let Fields::Named(named) = fields else {
        return quote!(#enum_ident::#variant_ident);
    };
    let field_idents: Vec<&Ident> = named
        .named
        .iter()
        .map(|f| f.ident.as_ref().expect("named field"))
        .collect();
    quote! {
        #enum_ident::#variant_ident {
            #( #field_idents: ::core::default::Default::default(), )*
        }
    }
}
```

- [ ] **Step 4: Wire the helper into `OsSubMenu`**

In `crates/vmux_macro/src/lib.rs`, locate the `impl_os_sub_menu` function and the `Fields::Unit` guard around line 73. Replace:

```rust
let Fields::Unit = &variant.fields else {
    return Err(syn::Error::new_spanned(
        variant,
        "OsSubMenu expects unit-style variants",
    ));
};
```

with:

```rust
let constructor = match &variant.fields {
    syn::Fields::Unit => quote::quote!(#enum_ident::#variant_ident),
    syn::Fields::Named(_) => named_fields::build_default_named_constructor(
        &enum_ident,
        variant_ident,
        &variant.fields,
    ),
    syn::Fields::Unnamed(_) => {
        return Err(syn::Error::new_spanned(
            variant,
            "OsSubMenu leaf variants must be Unit or named-field",
        ));
    }
};
```

Then use `constructor` wherever the existing code wrote `#enum_ident::#variant_ident` to construct the variant for menu-id dispatch.

Add the `mod named_fields;` line at the top of `lib.rs`.

- [ ] **Step 5: Run the test to verify it passes**

Run: `bash -c "cargo test -p vmux_macro --test named_field_variants 2>&1 | tail -30"`
Expected: PASS — both tests green.

- [ ] **Step 6: Run lint + existing tests on changed crate**

Run: `bash -c "cargo fmt -p vmux_macro -- --check && env -u CEF_PATH cargo clippy -p vmux_macro --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_macro 2>&1 | tail -20"`
Expected: green across fmt, clippy, all tests.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_macro/src/named_fields.rs crates/vmux_macro/src/lib.rs crates/vmux_macro/tests/named_field_variants.rs
git commit -m "feat(vmux_macro): OsSubMenu accepts Fields::Named (VMX-124)"
```

---

## Task 2: Macro — accept `Fields::Named` in `DefaultShortcuts`

**Files:**
- Modify: `crates/vmux_macro/src/lib.rs` (the `impl_default_shortcuts` function, around line 394)
- Test: `crates/vmux_macro/tests/named_field_shortcuts.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/vmux_macro/tests/named_field_shortcuts.rs
use vmux_macro::{DefaultShortcuts, OsSubMenu};

#[derive(OsSubMenu, DefaultShortcuts, Debug, Clone, PartialEq, Eq, Default)]
enum Sample {
    #[default]
    #[menu(id = "sample_a", label = "Sample A", accel = "super+k")]
    A { url: Option<String> },
}

#[test]
fn default_shortcuts_includes_named_field_variant() {
    let pairs: Vec<_> = Sample::default_shortcuts();
    assert!(pairs.iter().any(|(_, id)| id == "sample_a"));
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `bash -c "cargo test -p vmux_macro --test named_field_shortcuts 2>&1 | tail -20"`
Expected: compile error — `DefaultShortcuts` derive rejects named fields.

- [ ] **Step 3: Update `DefaultShortcuts` leaf-handling**

In `lib.rs`, find the helper that filters variants for shortcut emission (around line 394). The current line:

```rust
.map(|v| matches!(v.fields, Fields::Unit))
```

is used to gate which variants get shortcuts. Replace any `Fields::Unit` *gates* with `matches!(v.fields, Fields::Unit | Fields::Named(_))` for `DefaultShortcuts` only — the macro should not reject named-field variants here.

At the actual shortcut-table generation site (inside `impl_default_shortcuts_leaf`), where the macro currently emits `(shortcut, #id)`, the per-variant ID emission is already independent of the variant body. Keep that. No need to instantiate the variant for shortcut metadata — only the menu_id string is recorded.

- [ ] **Step 4: Run the test to verify it passes**

Run: `bash -c "cargo test -p vmux_macro --test named_field_shortcuts 2>&1 | tail -20"`
Expected: PASS.

- [ ] **Step 5: Run lint + tests**

Run: `bash -c "cargo fmt -p vmux_macro -- --check && env -u CEF_PATH cargo clippy -p vmux_macro --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_macro 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_macro/src/lib.rs crates/vmux_macro/tests/named_field_shortcuts.rs
git commit -m "feat(vmux_macro): DefaultShortcuts accepts Fields::Named (VMX-124)"
```

---

## Task 3: Macro — accept `Fields::Named` in `CommandBar`

**Files:**
- Modify: `crates/vmux_macro/src/lib.rs` (the `impl_command_bar` function, around line 663)
- Test: `crates/vmux_macro/tests/named_field_command_bar.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/vmux_macro/tests/named_field_command_bar.rs
use vmux_macro::{CommandBar, OsSubMenu};

#[derive(OsSubMenu, CommandBar, Debug, Clone, PartialEq, Eq, Default)]
enum Sample {
    #[default]
    #[menu(id = "sample_a", label = "Sample A")]
    A { url: Option<String> },
}

#[test]
fn command_bar_entries_include_named_field_variant() {
    let entries: Vec<_> = Sample::command_bar_entries();
    assert!(entries.iter().any(|(id, _, _)| *id == "sample_a"));
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `bash -c "cargo test -p vmux_macro --test named_field_command_bar 2>&1 | tail -20"`
Expected: compile error.

- [ ] **Step 3: Update `CommandBar` leaf-handling**

Mirror the change from Task 2 — at the gate around line 663 that currently does `matches!(v.fields, Fields::Unit)`, broaden to accept `Fields::Named` as well.

- [ ] **Step 4: Run the test to verify it passes**

Run: `bash -c "cargo test -p vmux_macro --test named_field_command_bar 2>&1 | tail -20"`
Expected: PASS.

- [ ] **Step 5: Run lint + tests**

Run: `bash -c "cargo fmt -p vmux_macro -- --check && env -u CEF_PATH cargo clippy -p vmux_macro --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_macro 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_macro/src/lib.rs crates/vmux_macro/tests/named_field_command_bar.rs
git commit -m "feat(vmux_macro): CommandBar accepts Fields::Named (VMX-124)"
```

---

## Task 4: Macro — `#[shortcut(chord = ..., variant = "...")]` for explicit-variant chord binding

**Files:**
- Modify: `crates/vmux_macro/src/lib.rs` (extend `ShortcutProps` parsing inside `impl_default_shortcuts_leaf`)
- Test: `crates/vmux_macro/tests/explicit_variant_chord.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/vmux_macro/tests/explicit_variant_chord.rs
use vmux_macro::{DefaultShortcuts, OsSubMenu};

#[derive(OsSubMenu, DefaultShortcuts, Debug, Clone, PartialEq, Eq, Default)]
enum Sample {
    #[default]
    #[menu(id = "sample_pane", label = "Pane")]
    #[shortcut(
        chord = "Ctrl+g, %",
        variant = "Pane { direction: Right }"
    )]
    Pane { direction: Direction },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction { #[default] Right, Left }

#[test]
fn explicit_variant_chord_registers_under_specified_instantiation() {
    let chords: Vec<_> = Sample::extra_chord_bindings();
    let found = chords.iter().any(|(chord, variant)| {
        chord == "Ctrl+g, %" && matches!(variant, Sample::Pane { direction: Direction::Right })
    });
    assert!(found, "expected Ctrl+g, % bound to Pane {{ direction: Right }}");
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `bash -c "cargo test -p vmux_macro --test explicit_variant_chord 2>&1 | tail -20"`
Expected: compile error — neither `chord = "..."` nor `variant = "..."` is parsed; `extra_chord_bindings` doesn't exist yet.

- [ ] **Step 3: Parse the new attribute fields**

Inside the existing `ShortcutProps::from_attrs` helper in `lib.rs`, extend the per-attribute meta walker to recognise two additional keys: `chord` (string literal — the chord notation) and `variant` (string literal — a Rust expression after the enum name). Collect these into a new `Vec<(String, String)> extra_chords` on `ShortcutProps`.

- [ ] **Step 4: Emit the `extra_chord_bindings` method**

In `impl_default_shortcuts_leaf`, after the main shortcut table emission, add:

```rust
let extra_entries: Vec<_> = data.variants.iter()
    .flat_map(|v| {
        let props = ShortcutProps::from_attrs(&v.attrs).unwrap_or_default();
        props.extra_chords.into_iter().map(move |(chord_str, variant_expr_str)| {
            let chord = parse_chord(&chord_str);
            // variant_expr_str is "Pane { direction: Right }" — splice it after the enum ident
            let variant_expr: syn::Expr = syn::parse_str(&variant_expr_str)
                .expect("invalid variant expression in #[shortcut(variant = ...)]");
            quote! { ( #chord, #enum_ident::#variant_expr ) }
        })
    })
    .collect();

let extra_method = quote! {
    impl #enum_ident {
        pub fn extra_chord_bindings() -> Vec<(crate::shortcut::Shortcut, Self)> {
            vec![#(#extra_entries),*]
        }
    }
};
```

Splice `extra_method` into the macro's output `TokenStream`.

(`parse_chord` is the same helper the existing `#[shortcut(chord = "...")]` attribute already uses for the unit-variant case — reuse it.)

- [ ] **Step 5: Run the test to verify it passes**

Run: `bash -c "cargo test -p vmux_macro --test explicit_variant_chord 2>&1 | tail -20"`
Expected: PASS.

- [ ] **Step 6: Run lint + tests**

Run: `bash -c "cargo fmt -p vmux_macro -- --check && env -u CEF_PATH cargo clippy -p vmux_macro --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_macro 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_macro/src/lib.rs crates/vmux_macro/tests/explicit_variant_chord.rs
git commit -m "feat(vmux_macro): #[shortcut(chord, variant)] binds chord to explicit variant instantiation (VMX-124)"
```

---

## Task 5: Macro — `#[menu(expand = "<field>")]` direction fan-out

**Files:**
- Modify: `crates/vmux_macro/src/lib.rs` (extend `MenuProps`, fan-out emission in OsSubMenu / DefaultShortcuts / CommandBar)
- Create: `crates/vmux_macro/src/expand.rs`
- Test: `crates/vmux_macro/tests/expand_direction.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/vmux_macro/tests/expand_direction.rs
use vmux_macro::{CommandBar, DefaultShortcuts, OsSubMenu};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaneDirection {
    #[default]
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, PartialEq, Eq, Default)]
enum Sample {
    #[default]
    #[menu(
        expand = "direction",
        id_template = "sample_in_pane_{dir}",
        label_template = "In Pane {Dir}",
    )]
    #[shortcut(
        expand = "direction",
        top = "Super+Shift+K",
        right = "Super+Shift+L",
        bottom = "Super+Shift+J",
        left = "Super+Shift+H",
    )]
    InPane { direction: PaneDirection, url: Option<String> },
}

#[test]
fn expand_generates_four_menu_ids() {
    for dir in ["top", "right", "bottom", "left"] {
        let id = format!("sample_in_pane_{dir}");
        assert!(
            Sample::from_menu_id(&id).is_some(),
            "expected {id} to resolve via from_menu_id",
        );
    }
}

#[test]
fn expand_generates_four_shortcuts() {
    let shortcuts = Sample::default_shortcuts();
    let ids: Vec<_> = shortcuts.iter().map(|(_, id)| id.clone()).collect();
    assert!(ids.contains(&"sample_in_pane_top".to_string()));
    assert!(ids.contains(&"sample_in_pane_right".to_string()));
    assert!(ids.contains(&"sample_in_pane_bottom".to_string()));
    assert!(ids.contains(&"sample_in_pane_left".to_string()));
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `bash -c "cargo test -p vmux_macro --test expand_direction 2>&1 | tail -30"`
Expected: compile error — `expand` / `id_template` / `label_template` not recognised.

- [ ] **Step 3: Add the expand helper module**

```rust
// crates/vmux_macro/src/expand.rs
use heck::{ToPascalCase, ToSnakeCase};
use syn::Ident;

/// Hardcoded enumeration of supported expand-field types. Add new entries here
/// when a new bounded enum needs `#[menu(expand = ...)]` support. The macro
/// can't read other crates' enum definitions at expansion time, so the
/// supported set is explicit.
pub fn variants_for(field_type: &Ident) -> Option<&'static [&'static str]> {
    match field_type.to_string().as_str() {
        "PaneDirection" => Some(&["Top", "Right", "Bottom", "Left"]),
        _ => None,
    }
}

pub fn format_id_template(template: &str, variant_pascal: &str) -> String {
    template.replace("{dir}", &variant_pascal.to_snake_case())
}

pub fn format_label_template(template: &str, variant_pascal: &str) -> String {
    template
        .replace("{Dir}", &variant_pascal.to_pascal_case())
        .replace("{dir}", &variant_pascal.to_snake_case())
}
```

Add `mod expand;` and `use expand::*;` in `lib.rs`.

- [ ] **Step 4: Extend `MenuProps` + `ShortcutProps`**

In `lib.rs`, add fields:

```rust
struct MenuProps {
    // existing fields ...
    expand: Option<String>,        // field name to expand over
    id_template: Option<String>,
    label_template: Option<String>,
}

struct ShortcutProps {
    // existing fields ...
    expand: Option<String>,
    direction_keys: Vec<(String, String)>, // (variant lowercase, chord string)
    extra_chords: Vec<(String, String)>,   // from Task 4
}
```

Parse the new keys in `from_attrs` for both helpers. For `#[shortcut(expand = "direction", top = "...", right = "...", ...)]`, the per-direction keys go into `direction_keys`.

- [ ] **Step 5: Emit fan-out**

In the OsSubMenu / DefaultShortcuts / CommandBar leaf walkers, when a variant has `expand = Some(field)`:

```rust
if let Some(field_name) = &menu_props.expand {
    let field_type = lookup_field_type(&variant.fields, field_name)
        .ok_or_else(|| syn::Error::new_spanned(variant, "expand field not found"))?;
    let variants = expand::variants_for(&field_type)
        .ok_or_else(|| syn::Error::new_spanned(variant, "unsupported expand type"))?;
    for variant_name_str in variants {
        let id_str = expand::format_id_template(
            menu_props.id_template.as_deref().expect("id_template required with expand"),
            variant_name_str,
        );
        let label_str = expand::format_label_template(
            menu_props.label_template.as_deref().expect("label_template required with expand"),
            variant_name_str,
        );
        let variant_pascal = syn::Ident::new(variant_name_str, variant.ident.span());
        // emit a menu-id arm: id_str => Self::InPane { direction: PaneDirection::Right, ..Default::default() }
        let field_type_path: syn::Path = syn::parse_str(&field_type.to_string())?;
        from_menu_id_arms.push(quote! {
            #id_str => Some(#enum_ident::#variant_ident {
                #field_name_ident: #field_type_path::#variant_pascal,
                ..::core::default::Default::default()
            }),
        });
        // (similar additions for shortcut table + command_bar_entries)
    }
} else {
    // existing single-variant emission
}
```

Repeat the equivalent in `impl_default_shortcuts_leaf` (using `direction_keys` to pick the chord per direction) and `impl_command_bar_leaf` (one entry per direction).

- [ ] **Step 6: Run the test to verify it passes**

Run: `bash -c "cargo test -p vmux_macro --test expand_direction 2>&1 | tail -30"`
Expected: PASS.

- [ ] **Step 7: Run lint + all macro tests**

Run: `bash -c "cargo fmt -p vmux_macro -- --check && env -u CEF_PATH cargo clippy -p vmux_macro --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_macro 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_macro/src/expand.rs crates/vmux_macro/src/lib.rs crates/vmux_macro/tests/expand_direction.rs
git commit -m "feat(vmux_macro): #[menu(expand)] + #[shortcut(expand)] fan out variants by bounded enum field (VMX-124)"
```

---

## Task 6: Define `OpenCommand` enum + supporting types

**Files:**
- Create: `crates/vmux_command/src/open.rs`
- Modify: `crates/vmux_command/src/lib.rs`
- Test: `crates/vmux_command/tests/open_command_derives.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/vmux_command/tests/open_command_derives.rs
use vmux_command::open::{OpenCommand, PaneDirection, PaneOpenMode, PaneTarget};

#[test]
fn default_pane_target_is_new_split() {
    assert_eq!(PaneTarget::default(), PaneTarget::NewSplit);
}

#[test]
fn default_pane_open_mode_is_new_stack() {
    assert_eq!(PaneOpenMode::default(), PaneOpenMode::NewStack);
}

#[test]
fn open_command_in_place_has_none_url_default() {
    let cmd = OpenCommand::InPlace { url: None };
    assert!(matches!(cmd, OpenCommand::InPlace { url: None }));
}

#[test]
fn open_command_in_pane_carries_all_four_fields() {
    let cmd = OpenCommand::InPane {
        direction: PaneDirection::Right,
        target: PaneTarget::Existing,
        mode: PaneOpenMode::InPlace,
        url: Some("https://example.com".to_string()),
    };
    let OpenCommand::InPane { direction, target, mode, url } = cmd else {
        panic!("expected InPane variant");
    };
    assert_eq!(direction, PaneDirection::Right);
    assert_eq!(target, PaneTarget::Existing);
    assert_eq!(mode, PaneOpenMode::InPlace);
    assert_eq!(url.as_deref(), Some("https://example.com"));
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `bash -c "cargo test -p vmux_command --test open_command_derives 2>&1 | tail -20"`
Expected: module `open` not found.

- [ ] **Step 3: Implement the module**

```rust
// crates/vmux_command/src/open.rs
use vmux_macro::{CommandBar, DefaultShortcuts, McpTool, OsSubMenu};

#[derive(
    OsSubMenu, DefaultShortcuts, CommandBar, McpTool,
    Debug, Clone, PartialEq, Eq, Default,
)]
pub enum OpenCommand {
    #[default]
    #[menu(id = "open_in_place", label = "Open Here", accel = "super+l")]
    #[mcp(description = "Navigate the currently focused stack to the URL (or the startup URL if omitted).")]
    InPlace {
        #[mcp(description = "Absolute URL to open. If omitted, opens the startup URL.")]
        url: Option<String>,
    },

    #[menu(id = "open_in_new_stack", label = "Open in New Stack", accel = "super+n")]
    #[mcp(description = "Open the URL as a new stack inside the currently focused pane.")]
    InNewStack {
        #[mcp(description = "Absolute URL to open. If omitted, opens the startup URL.")]
        url: Option<String>,
    },

    #[menu(
        expand = "direction",
        id_template = "open_in_pane_{dir}",
        label_template = "Open in Pane {Dir}",
    )]
    #[shortcut(
        expand = "direction",
        top = "Super+Shift+K",
        right = "Super+Shift+L",
        bottom = "Super+Shift+J",
        left = "Super+Shift+H",
    )]
    #[shortcut(chord = "Ctrl+g, %", variant = "InPane { direction: PaneDirection::Right, target: PaneTarget::NewSplit, mode: PaneOpenMode::NewStack, url: None }")]
    #[shortcut(chord = "Ctrl+g, \"", variant = "InPane { direction: PaneDirection::Bottom, target: PaneTarget::NewSplit, mode: PaneOpenMode::NewStack, url: None }")]
    #[mcp(description = "Open URL in a sibling pane in the given direction. Set target=NewSplit to split the current pane, target=Existing to reuse an adjacent pane (falls back to NewSplit if none). Set mode=InPlace to navigate the chosen pane's active stack, mode=NewStack to add a stack to it.")]
    InPane {
        #[mcp(description = "Which side of the current pane to act on.", enum_values = ["top", "right", "bottom", "left"])]
        direction: PaneDirection,
        #[mcp(description = "Existing reuses the sibling pane in `direction` (falls back to NewSplit if none). NewSplit always splits the current pane.", enum_values = ["existing", "new_split"])]
        target: PaneTarget,
        #[mcp(description = "InPlace navigates the chosen pane's active stack. NewStack appends a new stack within that pane.", enum_values = ["in_place", "new_stack"])]
        mode: PaneOpenMode,
        #[mcp(description = "Absolute URL to open. If omitted, opens the startup URL.")]
        url: Option<String>,
    },

    #[menu(id = "open_in_new_tab", label = "Open in New Tab", accel = "super+t")]
    #[mcp(description = "Open URL in a brand-new Tab within the current Space.")]
    InNewTab {
        #[mcp(description = "Absolute URL to open. If omitted, opens the startup URL.")]
        url: Option<String>,
    },

    #[menu(id = "open_in_new_space", label = "Open in New Space", accel = "super+shift+n")]
    #[mcp(description = "Open URL in a brand-new Space (top-level profile).")]
    InNewSpace {
        #[mcp(description = "Absolute URL to open. If omitted, opens the startup URL.")]
        url: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum PaneDirection {
    #[default]
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum PaneTarget {
    Existing,
    #[default]
    NewSplit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum PaneOpenMode {
    InPlace,
    #[default]
    NewStack,
}

/// Sent across the webview IPC boundary in CommandBarOpenEvent.
/// Mirrors the variants of OpenCommand without the URL payload — the URL is
/// supplied by the user typing in the command bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum OpenTarget {
    #[default]
    InPlace,
    InNewStack,
    InPane { direction: PaneDirection, target: PaneTarget, mode: PaneOpenMode },
    InNewTab,
    InNewSpace,
}
```

Update `crates/vmux_command/src/lib.rs`:

```rust
pub mod open;
pub use open::*;
```

(Also add the `expand` module to the macro's supported list by adding `OpenCommand`'s `PaneDirection` to `crates/vmux_macro/src/expand.rs`'s `variants_for` — already done in Task 5 since we used the `PaneDirection` token name. Verify.)

- [ ] **Step 4: Run the test to verify it passes**

Run: `bash -c "cargo test -p vmux_command --test open_command_derives 2>&1 | tail -20"`
Expected: PASS.

- [ ] **Step 5: Run lint + all vmux_command tests**

Run: `bash -c "cargo fmt -p vmux_command -- --check && env -u CEF_PATH cargo clippy -p vmux_command --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_command 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_command/src/open.rs crates/vmux_command/src/lib.rs crates/vmux_command/tests/open_command_derives.rs
git commit -m "feat(vmux_command): add OpenCommand enum + PaneDirection/Target/Mode types (VMX-124)"
```

---

## Task 7: Add `resolve_url` helper

**Files:**
- Create: `crates/vmux_command/src/open/handler.rs`
- Modify: `crates/vmux_command/src/open.rs` (add `pub mod handler;`)
- Test: inline in `handler.rs`

- [ ] **Step 1: Write the failing test (inline at bottom of handler.rs)**

```rust
// crates/vmux_command/src/open/handler.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_url_prefers_explicit_url() {
        let resolved = resolve_url(Some("https://explicit"), Some("https://startup"));
        assert_eq!(resolved, "https://explicit");
    }

    #[test]
    fn resolve_url_falls_back_to_startup_when_none() {
        let resolved = resolve_url(None, Some("https://startup"));
        assert_eq!(resolved, "https://startup");
    }

    #[test]
    fn resolve_url_empty_string_is_treated_as_none() {
        let resolved = resolve_url(Some(""), Some("https://startup"));
        assert_eq!(resolved, "https://startup");
    }

    #[test]
    fn resolve_url_default_when_neither_provided() {
        let resolved = resolve_url(None, None);
        assert_eq!(resolved, DEFAULT_NEW_PAGE_URL);
    }
}
```

- [ ] **Step 2: Run the test (should fail to compile)**

Run: `bash -c "cargo test -p vmux_command 2>&1 | tail -20"`
Expected: module `handler` not found.

- [ ] **Step 3: Implement**

```rust
// crates/vmux_command/src/open/handler.rs
pub const DEFAULT_NEW_PAGE_URL: &str = "vmux://new-page/";

pub fn resolve_url(cmd_url: Option<&str>, startup_url: Option<&str>) -> String {
    cmd_url
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| startup_url.map(|s| s.to_string()))
        .unwrap_or_else(|| DEFAULT_NEW_PAGE_URL.to_string())
}

#[cfg(test)]
mod tests { /* as in Step 1 */ }
```

Update `crates/vmux_command/src/open.rs` to add `pub mod handler;` at the bottom (or convert `open.rs` into a directory pattern with the existing filename-based module convention).

Per project rule: use the filename-based module pattern. So:
- `crates/vmux_command/src/open.rs` defines the enum
- `crates/vmux_command/src/open/handler.rs` is the submodule

To make this work without `mod.rs`, add to `open.rs`:
```rust
pub mod handler;
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `bash -c "cargo test -p vmux_command --lib open::handler::tests 2>&1 | tail -20"`
Expected: all 4 tests PASS.

- [ ] **Step 5: Run lint + full crate tests**

Run: `bash -c "cargo fmt -p vmux_command -- --check && env -u CEF_PATH cargo clippy -p vmux_command --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_command 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_command/src/open.rs crates/vmux_command/src/open/handler.rs
git commit -m "feat(vmux_command): add resolve_url helper for OpenCommand handlers (VMX-124)"
```

---

## Task 8: Restructure `BrowserCommand` into OsSubMenuGroup

**Files:**
- Modify: `crates/vmux_command/src/command.rs` (split `BrowserCommand` into Navigation / Open / View / Bar sub-enums)
- Test: `crates/vmux_command/src/command.rs` (extend existing `tests` mod)

- [ ] **Step 1: Write the failing test**

Add to the existing `mod tests` block in `command.rs`:

```rust
#[test]
fn browser_open_in_new_stack_resolves_through_nested_chain() {
    assert!(matches!(
        AppCommand::from_menu_id("open_in_new_stack"),
        Some(AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack { url: None })))
    ));
}

#[test]
fn browser_navigation_back_still_resolves() {
    assert!(matches!(
        AppCommand::from_menu_id("browser_prev_page"),
        Some(AppCommand::Browser(BrowserCommand::Navigation(BrowserNavigationCommand::PrevPage)))
    ));
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `bash -c "cargo test -p vmux_command --lib tests 2>&1 | tail -20"`
Expected: compile errors — `BrowserCommand::Open` doesn't exist; `BrowserNavigationCommand` undefined.

- [ ] **Step 3: Split BrowserCommand**

Replace the existing flat `BrowserCommand` in `crates/vmux_command/src/command.rs` with:

```rust
#[derive(OsSubMenuGroup, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserCommand {
    #[menu(label = "Navigation")]
    Navigation(BrowserNavigationCommand),

    #[menu(label = "Open")]
    Open(OpenCommand),

    #[menu(label = "View")]
    View(BrowserViewCommand),

    #[menu(label = "Bar")]
    Bar(BrowserBarCommand),
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserNavigationCommand {
    #[default]
    #[menu(id = "browser_prev_page", label = "Back", accel = "super+[")]
    PrevPage,
    #[menu(id = "browser_next_page", label = "Forward", accel = "super+]")]
    NextPage,
    #[menu(id = "browser_reload", label = "Reload", accel = "super+r")]
    Reload,
    #[menu(id = "browser_hard_reload", label = "Hard Reload", accel = "super+shift+r")]
    HardReload,
    #[menu(id = "browser_stop", label = "Stop Loading", accel = "super+.", hidden)]
    Stop,
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserViewCommand {
    #[default]
    #[menu(id = "browser_zoom_in", label = "Zoom In", accel = "super+=")]
    ZoomIn,
    #[menu(id = "browser_zoom_out", label = "Zoom Out", accel = "super+-")]
    ZoomOut,
    #[menu(id = "browser_zoom_reset", label = "Actual Size", accel = "super+0")]
    ZoomReset,
    #[menu(id = "browser_dev_tools", label = "Developer Tools", accel = "super+alt+i")]
    DevTools,
    #[menu(id = "browser_view_source", label = "View Source", accel = "super+alt+u", hidden)]
    ViewSource,
    #[menu(id = "browser_print", label = "Print", accel = "super+p", hidden)]
    Print,
}

#[derive(OsSubMenu, DefaultShortcuts, CommandBar, McpTool, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserBarCommand {
    #[default]
    #[menu(id = "browser_open_command_bar", label = "Command Bar", accel = "super+k")]
    OpenCommandBar,
    #[menu(id = "browser_open_path_bar", label = "Path Navigator", accel = "super+/")]
    OpenPathBar,
    #[menu(id = "browser_open_commands", label = "Commands")]
    #[shortcut(direct = ">")]
    OpenCommands,
    #[menu(id = "browser_find", label = "Find", accel = "super+f", hidden)]
    Find,
}
```

Note: `BrowserCommand::FocusAddressBar` is **dropped** here — Task 13 will rebind `super+l` to `OpenCommand::InPlace`.

Add at top of file: `use crate::open::OpenCommand;`

- [ ] **Step 4: Run the tests to verify they pass**

Run: `bash -c "cargo test -p vmux_command --lib tests 2>&1 | tail -30"`
Expected: both new tests PASS, plus all existing `from_menu_id` tests stay green.

- [ ] **Step 5: Run lint + tests**

Run: `bash -c "cargo fmt -p vmux_command -- --check && env -u CEF_PATH cargo clippy -p vmux_command --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_command 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_command/src/command.rs
git commit -m "refactor(vmux_command): split BrowserCommand into Navigation/Open/View/Bar sub-enums (VMX-124)"
```

---

## Task 9: Implement `OpenCommand::InPlace` handler

**Files:**
- Modify: `crates/vmux_layout/src/stack.rs` (add handler)
- Test: `crates/vmux_layout/tests/open_in_place_handler.rs`

- [ ] **Step 1: Write the failing test**

The test scaffold mirrors the existing handler-test pattern in `crates/vmux_layout/src/pane.rs:2030-2060` (the `PaneCommand::SplitH` test): build a minimal `App`, register the systems under test, spawn a Space → Pane → Stack hierarchy, write the command via `Messages<AppCommand>`, call `app.update()`, then assert on world state.

```rust
// crates/vmux_layout/tests/open_in_place_handler.rs
use bevy::prelude::*;
use vmux_command::open::{OpenCommand};
use vmux_command::{AppCommand, BrowserCommand};
use vmux_layout::{settings::EffectiveStartupUrl, stack::handle_open_in_place_command};

fn build_test_app() -> (App, Entity /* stack */) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<AppCommand>()
        .init_resource::<vmux_layout::stack::FocusedStack>()
        .add_systems(Update, handle_open_in_place_command);

    // Spawn a Browser-bearing Stack and mark it focused.
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::Stack::default(), /* Browser component default */))
        .id();
    app.world_mut()
        .resource_mut::<vmux_layout::stack::FocusedStack>()
        .stack = Some(stack);
    (app, stack)
}

fn read_browser_url(app: &App, stack: Entity) -> String {
    // Adjust to the actual Browser API in vmux_layout::cef::Browser.
    app.world()
        .get::<vmux_layout::cef::Browser>(stack)
        .map(|b| b.current_url())
        .unwrap_or_default()
}

#[test]
fn in_place_with_explicit_url_navigates_active_stack() {
    let (mut app, stack) = build_test_app();
    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InPlace { url: Some("https://example.com".into()) },
        )));
    app.update();
    assert_eq!(read_browser_url(&app, stack), "https://example.com");
}

#[test]
fn in_place_with_none_url_uses_startup_setting() {
    let (mut app, stack) = build_test_app();
    app.world_mut().insert_resource(EffectiveStartupUrl("https://startup.example".into()));
    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InPlace { url: None },
        )));
    app.update();
    assert_eq!(read_browser_url(&app, stack), "https://startup.example");
}
```

If the actual `Browser` component API differs from `b.current_url()`, adjust the read helper to match. Look at `crates/vmux_layout/src/cef.rs` for the real accessor.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash -c "cargo test -p vmux_layout --test open_in_place_handler 2>&1 | tail -30"`
Expected: handler doesn't exist; compile error or panicking `todo!`.

- [ ] **Step 3: Implement the handler**

In `crates/vmux_layout/src/stack.rs`, add a new system:

```rust
pub fn handle_open_in_place_command(
    mut messages: MessageReader<AppCommand>,
    focused: Res<FocusedStack>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    mut browsers: Query<&mut Browser>,
) {
    for msg in messages.read() {
        let AppCommand::Browser(BrowserCommand::Open(OpenCommand::InPlace { url })) = msg else {
            continue;
        };
        let Some(stack_e) = focused.stack else { continue };
        let Ok(mut browser) = browsers.get_mut(stack_e) else { continue };
        let resolved = vmux_command::open::handler::resolve_url(
            url.as_deref(),
            effective_startup_url.as_ref().map(|s| s.0.as_str()),
        );
        browser.navigate(&resolved);
    }
}
```

Register it in `StackPlugin::build`:

```rust
.add_systems(
    Update,
    handle_open_in_place_command
        .in_set(ReadAppCommands)
        .after(ComputeFocusSet),
)
```

- [ ] **Step 4: Run the tests**

Run: `bash -c "cargo test -p vmux_layout --test open_in_place_handler 2>&1 | tail -30"`
Expected: PASS.

- [ ] **Step 5: Run lint + tests**

Run: `bash -c "cargo fmt -p vmux_layout -- --check && env -u CEF_PATH cargo clippy -p vmux_layout --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_layout 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/stack.rs crates/vmux_layout/tests/open_in_place_handler.rs
git commit -m "feat(vmux_layout): handler for OpenCommand::InPlace (VMX-124)"
```

---

## Task 10: Implement `OpenCommand::InNewStack` handler

**Files:**
- Modify: `crates/vmux_layout/src/stack.rs`
- Test: `crates/vmux_layout/tests/open_in_new_stack_handler.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/vmux_layout/tests/open_in_new_stack_handler.rs
// Build App, spawn Space→Pane→Stack, focus the stack, emit
// AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack { url: Some("https://example.com".into()) })),
// run one Update tick. Assert the active pane now has 2 stack children, the new
// one is active, and its Browser points at example.com.
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `bash -c "cargo test -p vmux_layout --test open_in_new_stack_handler 2>&1 | tail -30"`
Expected: handler missing.

- [ ] **Step 3: Implement**

In `stack.rs`, add `handle_open_in_new_stack_command`. Reuse the existing `spawn_stack_in_pane` helper currently called by `StackCommand::New`'s handler arm (around line 189) — extract that helper if not already a standalone function.

```rust
pub fn handle_open_in_new_stack_command(
    mut commands: Commands,
    mut messages: MessageReader<AppCommand>,
    focused: Res<FocusedStack>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
) {
    for msg in messages.read() {
        let AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack { url })) = msg else {
            continue;
        };
        let Some(pane_e) = focused.pane else { continue };
        let resolved = vmux_command::open::handler::resolve_url(
            url.as_deref(),
            effective_startup_url.as_ref().map(|s| s.0.as_str()),
        );
        commands.spawn((
            stack_bundle(),
            LastActivatedAt::now(),
            ChildOf(pane_e),
            // attach Browser w/ URL ...
        ));
    }
}
```

Register the system in `StackPlugin::build`.

- [ ] **Step 4: Run the tests**

Run: `bash -c "cargo test -p vmux_layout --test open_in_new_stack_handler 2>&1 | tail -30"`
Expected: PASS.

- [ ] **Step 5: Run lint + tests**

Run: `bash -c "cargo fmt -p vmux_layout -- --check && env -u CEF_PATH cargo clippy -p vmux_layout --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_layout 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/stack.rs crates/vmux_layout/tests/open_in_new_stack_handler.rs
git commit -m "feat(vmux_layout): handler for OpenCommand::InNewStack (VMX-124)"
```

---

## Task 11: Implement `OpenCommand::InPane` handler (all 4 combos + fallback)

**Files:**
- Modify: `crates/vmux_layout/src/pane.rs`
- Test: `crates/vmux_layout/tests/open_in_pane_handler.rs`

- [ ] **Step 1: Write the failing tests**

```rust
// crates/vmux_layout/tests/open_in_pane_handler.rs
use vmux_command::open::{OpenCommand, PaneDirection, PaneOpenMode, PaneTarget};
use vmux_command::{AppCommand, BrowserCommand};

#[test]
fn new_split_creates_pane_in_direction() {
    // Build App, spawn Space→Pane(single leaf)→Stack, focus the pane.
    // Emit InPane { Right, NewSplit, NewStack, Some(url) }.
    // Assert: pane tree now has a PaneSplit with two children, the new pane
    // is on the right, contains one Stack with browser at url.
}

#[test]
fn existing_with_in_place_navigates_active_stack_of_neighbour() {
    // Build App with two side-by-side panes (already split).
    // Focus left pane. Right pane has Stack at https://old.
    // Emit InPane { Right, Existing, InPlace, Some("https://new") }.
    // Assert: right pane's existing stack now points to https://new.
}

#[test]
fn existing_with_new_stack_adds_stack_to_neighbour() {
    // Build App with two side-by-side panes. Focus left.
    // Right pane has 1 Stack.
    // Emit InPane { Right, Existing, NewStack, Some(url) }.
    // Assert: right pane now has 2 stacks, new one active w/ url.
}

#[test]
fn existing_falls_back_to_new_split_when_no_sibling_in_direction() {
    // Build App with single pane. Emit InPane { Right, Existing, InPlace, Some(url) }.
    // Assert: pane was split; new pane on right contains url.
}
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `bash -c "cargo test -p vmux_layout --test open_in_pane_handler 2>&1 | tail -30"`
Expected: handler missing.

- [ ] **Step 3: Implement**

In `crates/vmux_layout/src/pane.rs`, add `handle_open_in_pane_command`. Reuse the existing split machinery from `handle_pane_commands` (the `PaneCommand::SplitV / SplitH` arms around line 514). Extract a helper:

```rust
fn split_current_pane(
    commands: &mut Commands,
    active_pane: Entity,
    direction: PaneDirection,
    // ... children/transform queries
) -> Entity {
    // existing logic, returns the new leaf pane entity
    todo!("extract from PaneCommand::SplitV/SplitH arm")
}

fn find_sibling_pane_in_direction(
    /* queries */
    active_pane: Entity,
    direction: PaneDirection,
) -> Option<Entity> {
    // walk the PaneSplit tree to find the leaf pane on the requested side
    None
}

pub fn handle_open_in_pane_command(
    mut commands: Commands,
    mut messages: MessageReader<AppCommand>,
    focused: Res<FocusedStack>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    // ... pane tree queries
) {
    for msg in messages.read() {
        let AppCommand::Browser(BrowserCommand::Open(OpenCommand::InPane {
            direction, target, mode, url,
        })) = msg
        else {
            continue;
        };
        let Some(active_pane) = focused.pane else { continue };
        let resolved = vmux_command::open::handler::resolve_url(
            url.as_deref(),
            effective_startup_url.as_ref().map(|s| s.0.as_str()),
        );

        let target_pane = match target {
            PaneTarget::Existing => find_sibling_pane_in_direction(/*...*/, active_pane, *direction)
                .unwrap_or_else(|| split_current_pane(&mut commands, active_pane, *direction, /*...*/)),
            PaneTarget::NewSplit => split_current_pane(&mut commands, active_pane, *direction, /*...*/),
        };

        match (target, mode) {
            (PaneTarget::NewSplit, _) => {
                // new pane already has 1 fresh stack — navigate it to resolved URL
                navigate_active_stack_of(target_pane, &resolved);
            }
            (PaneTarget::Existing, PaneOpenMode::InPlace) => {
                navigate_active_stack_of(target_pane, &resolved);
            }
            (PaneTarget::Existing, PaneOpenMode::NewStack) => {
                commands.spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(target_pane), /* browser w/ url */));
            }
        }
    }
}
```

Register in `PanePlugin::build`.

- [ ] **Step 4: Run the tests**

Run: `bash -c "cargo test -p vmux_layout --test open_in_pane_handler 2>&1 | tail -30"`
Expected: 4 tests PASS.

- [ ] **Step 5: Run lint + tests**

Run: `bash -c "cargo fmt -p vmux_layout -- --check && env -u CEF_PATH cargo clippy -p vmux_layout --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_layout 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/pane.rs crates/vmux_layout/tests/open_in_pane_handler.rs
git commit -m "feat(vmux_layout): handler for OpenCommand::InPane (VMX-124)"
```

---

## Task 12: Implement `OpenCommand::InNewTab` handler

**Files:**
- Modify: `crates/vmux_layout/src/space.rs`
- Test: `crates/vmux_layout/tests/open_in_new_tab_handler.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/vmux_layout/tests/open_in_new_tab_handler.rs
// Build App with one Space (the workspace-tab in the new model). Emit
// InNewTab { url: Some("https://example.com") }. Assert: a second Space
// entity now exists, is active, and its initial pane→stack→browser points at url.
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `bash -c "cargo test -p vmux_layout --test open_in_new_tab_handler 2>&1 | tail -30"`
Expected: handler missing.

- [ ] **Step 3: Implement**

In `crates/vmux_layout/src/space.rs`, add `handle_open_in_new_tab_command`. Reuse the existing `spawn_new_space` helper currently called by `TabCommand::New`'s handler arm (around line 181).

```rust
pub fn handle_open_in_new_tab_command(
    mut commands: Commands,
    mut messages: MessageReader<AppCommand>,
    main_q: Query<Entity, With<Main>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    spaces: Query<Entity, With<Space>>,
    settings: Res<LayoutSettings>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    mut new_stack_ctx: ResMut<NewStackContext>,
    mut spawn_requests: MessageWriter<LayoutSpawnRequest>,
) {
    for msg in messages.read() {
        let AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewTab { url })) = msg else {
            continue;
        };
        let Ok(main) = main_q.single() else { continue };
        let count = spaces.iter().count();
        let name = format!("Tab {}", count + 1);
        let resolved = vmux_command::open::handler::resolve_url(
            url.as_deref(),
            effective_startup_url.as_ref().map(|s| s.0.as_str()),
        );
        spawn_new_space(
            main,
            *primary_window,
            name,
            &settings,
            Some(&crate::settings::EffectiveStartupUrl(resolved)),
            &mut new_stack_ctx,
            &mut spawn_requests,
            &mut commands,
        );
    }
}
```

Register in `SpacePlugin::build`.

- [ ] **Step 4: Run the tests**

Run: `bash -c "cargo test -p vmux_layout --test open_in_new_tab_handler 2>&1 | tail -30"`
Expected: PASS.

- [ ] **Step 5: Run lint + tests**

Run: `bash -c "cargo fmt -p vmux_layout -- --check && env -u CEF_PATH cargo clippy -p vmux_layout --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_layout 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/space.rs crates/vmux_layout/tests/open_in_new_tab_handler.rs
git commit -m "feat(vmux_layout): handler for OpenCommand::InNewTab (VMX-124)"
```

---

## Task 13: Implement `OpenCommand::InNewSpace` handler

**Files:**
- Modify: `crates/vmux_layout/src/profile.rs` and/or `crates/vmux_layout/src/space.rs`
- Test: `crates/vmux_layout/tests/open_in_new_space_handler.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/vmux_layout/tests/open_in_new_space_handler.rs
// Build App with one Profile (= Space in new naming, 1:1). Emit
// InNewSpace { url: Some("https://example.com") }. Assert: a second Profile entity now
// exists, is active, and its default layout's stack points at url.
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `bash -c "cargo test -p vmux_layout --test open_in_new_space_handler 2>&1 | tail -30"`
Expected: handler missing.

- [ ] **Step 3: Implement**

`InNewSpace` is the new-profile operation. Since the entity rename (Profile → Space) is in flight, today's `Profile` entity is what the new model calls `Space`. Spawn a new `Profile` entity, populate its default layout (1 Tab → 1 Pane → 1 Stack), and navigate that stack to the resolved URL. Activate the new profile.

```rust
pub fn handle_open_in_new_space_command(
    mut commands: Commands,
    mut messages: MessageReader<AppCommand>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    // ... query for Profile / Active / etc
) {
    for msg in messages.read() {
        let AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewSpace { url })) = msg else {
            continue;
        };
        let resolved = vmux_command::open::handler::resolve_url(
            url.as_deref(),
            effective_startup_url.as_ref().map(|s| s.0.as_str()),
        );
        // Spawn new Profile + default layout. Navigate first stack to resolved.
        // Implementation detail: replicate window::spawn_default_session minus the
        // Profile::default_profile() call, parameterising with `resolved`.
    }
}
```

Register in `ProfilePlugin::build`.

- [ ] **Step 4: Run the tests**

Run: `bash -c "cargo test -p vmux_layout --test open_in_new_space_handler 2>&1 | tail -30"`
Expected: PASS.

- [ ] **Step 5: Run lint + tests**

Run: `bash -c "cargo fmt -p vmux_layout -- --check && env -u CEF_PATH cargo clippy -p vmux_layout --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_layout 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/profile.rs crates/vmux_layout/src/space.rs crates/vmux_layout/tests/open_in_new_space_handler.rs
git commit -m "feat(vmux_layout): handler for OpenCommand::InNewSpace (VMX-124)"
```

---

## Task 14: Migrate `CommandBarOpenEvent` IPC (replace `new_tab` with `target`)

**Files:**
- Modify: `crates/vmux_command/src/event.rs` (replace field)
- Modify: `crates/vmux_layout/src/command_bar/handler.rs` (read new field)
- Modify: `crates/vmux_layout/src/command_bar/page.rs` (webview side — send new field)
- Modify: `website/` / `crates/vmux_*` webview-side senders that build `CommandBarOpenEvent`
- Test: `crates/vmux_command/src/event.rs` (extend existing rkyv round-trip tests)

- [ ] **Step 1: Write the failing test**

In `crates/vmux_command/src/event.rs`'s `#[cfg(test)] mod tests`:

```rust
#[test]
fn command_bar_open_event_carries_target_enum() {
    let event = CommandBarOpenEvent {
        target: Some(crate::open::OpenTarget::InNewStack),
        ..Default::default()
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
    let recovered = rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");
    assert_eq!(recovered.target, Some(crate::open::OpenTarget::InNewStack));
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `bash -c "cargo test -p vmux_command --lib event::tests 2>&1 | tail -20"`
Expected: `target` field doesn't exist.

- [ ] **Step 3: Update `CommandBarOpenEvent`**

In `crates/vmux_command/src/event.rs`, replace:

```rust
pub new_tab: bool,
```

with:

```rust
#[serde(default)]
pub target: Option<crate::open::OpenTarget>,
```

Remove the old `new_tab` test entries that referenced the bool.

- [ ] **Step 4: Update host-side reader**

In `crates/vmux_layout/src/command_bar/handler.rs`, find every use of `evt.new_tab` and replace with `evt.target` matching the right `OpenTarget` variant. The existing `let is_new_stack = new_stack_ctx.stack.is_some();` derivation becomes `let is_new_stack = matches!(evt.target, Some(OpenTarget::InNewStack));`.

- [ ] **Step 5: Update webview-side sender**

In `crates/vmux_layout/src/command_bar/page.rs` (the Dioxus side), find calls that build `CommandBarOpenEvent` and update from `new_tab: bool` to `target: Option<OpenTarget>`.

- [ ] **Step 6: Run the test**

Run: `bash -c "cargo test -p vmux_command --lib event::tests 2>&1 | tail -20"`
Expected: PASS.

- [ ] **Step 7: Run lint + tests on changed crates**

Run: `bash -c "cargo fmt -p vmux_command -p vmux_layout -- --check && env -u CEF_PATH cargo clippy -p vmux_command -p vmux_layout --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_command -p vmux_layout 2>&1 | tail -30"`
Expected: green.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_command/src/event.rs crates/vmux_layout/src/command_bar/handler.rs crates/vmux_layout/src/command_bar/page.rs
git commit -m "refactor(ipc): CommandBarOpenEvent.new_tab -> target: Option<OpenTarget> (VMX-124)"
```

---

## Task 15: Replace `CommandBarActionEvent.action` navigate/new_tab dispatch with direct `OpenCommand` emit

**Files:**
- Modify: `crates/vmux_layout/src/command_bar/handler.rs` (the `on_command_bar_action` observer)
- Modify: `crates/vmux_layout/src/command_bar/page.rs` (webview side)
- Test: `crates/vmux_layout/tests/command_bar_navigate_emits_open.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/vmux_layout/tests/command_bar_navigate_emits_open.rs
// Build App. Trigger CommandBarActionEvent { action: "open", target: OpenTarget::InNewStack, value: "https://x" }.
// Assert: AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack { url: Some("https://x") })) was queued.
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `bash -c "cargo test -p vmux_layout --test command_bar_navigate_emits_open 2>&1 | tail -20"`
Expected: failure — current handler emits a string-typed action.

- [ ] **Step 3: Implement**

Replace `CommandBarActionEvent`'s `action: String` interpretation in `on_command_bar_action` with a typed dispatch. Concretely:

- Drop the `"navigate" | "new_tab"` string match arms.
- Emit `AppCommand::Browser(BrowserCommand::Open(open_cmd))` where `open_cmd` is constructed from the modal's `target` field (carried in `CommandBarOpenEvent` from Task 14) plus the `value` URL.

Helper:

```rust
fn build_open_command(target: OpenTarget, url: Option<String>) -> OpenCommand {
    match target {
        OpenTarget::InPlace => OpenCommand::InPlace { url },
        OpenTarget::InNewStack => OpenCommand::InNewStack { url },
        OpenTarget::InPane { direction, target: pt, mode } => OpenCommand::InPane {
            direction, target: pt, mode, url,
        },
        OpenTarget::InNewTab => OpenCommand::InNewTab { url },
        OpenTarget::InNewSpace => OpenCommand::InNewSpace { url },
    }
}
```

The remaining `action` field on `CommandBarActionEvent` collapses to just two values: `"open"` (the navigate dispatch above) and any other existing actions (e.g. `"focus-bar"` if such exists). Audit current callers; if `action` becomes single-valued, drop it entirely.

- [ ] **Step 4: Run the test**

Run: `bash -c "cargo test -p vmux_layout --test command_bar_navigate_emits_open 2>&1 | tail -20"`
Expected: PASS.

- [ ] **Step 5: Run lint + tests**

Run: `bash -c "cargo fmt -p vmux_layout -- --check && env -u CEF_PATH cargo clippy -p vmux_layout --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_layout 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/command_bar/handler.rs crates/vmux_layout/src/command_bar/page.rs
git commit -m "refactor(command_bar): navigate action emits OpenCommand directly (VMX-124)"
```

---

## Task 16: Remove `StackCommand::New` (its work now lives in `InNewStack` handler)

**Files:**
- Modify: `crates/vmux_command/src/command.rs` (drop `StackCommand::New` variant)
- Modify: `crates/vmux_layout/src/stack.rs` (drop the `StackCommand::New` arm — handler from Task 10 replaces it)
- Modify: any call site that wrote `StackCommand::New` — replace with `OpenCommand::InNewStack { url: None }`
- Test: extend existing tests in `command.rs` to assert `stack_new` no longer resolves

- [ ] **Step 1: Write the failing test**

In `command.rs` test mod:

```rust
#[test]
fn stack_new_id_no_longer_resolves() {
    assert!(AppCommand::from_menu_id("stack_new").is_none());
}
```

- [ ] **Step 2: Run the test (it should fail because stack_new still resolves)**

Run: `bash -c "cargo test -p vmux_command --lib tests::stack_new_id_no_longer_resolves 2>&1 | tail -10"`
Expected: assertion failure — `stack_new` currently resolves.

- [ ] **Step 3: Remove the variant**

In `command.rs`, drop the `#[menu(id = "stack_new", ...)] New` variant from `StackCommand`. Adjust the `#[default]` to a remaining variant (e.g. `Close`).

In `stack.rs`, drop the `StackCommand::New => { ... }` match arm inside `handle_stack_commands`.

Run grep to find any other callers:

```bash
bash -c "rg -n 'StackCommand::New' crates/"
```

Replace each with `AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack { url: None }))`.

- [ ] **Step 4: Run tests**

Run: `bash -c "cargo test -p vmux_command -p vmux_layout 2>&1 | tail -30"`
Expected: PASS, including the new `stack_new_id_no_longer_resolves` assertion.

- [ ] **Step 5: Run lint**

Run: `bash -c "cargo fmt -p vmux_command -p vmux_layout -- --check && env -u CEF_PATH cargo clippy -p vmux_command -p vmux_layout --all-targets -- -D warnings 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_command/src/command.rs crates/vmux_layout/src/stack.rs
git commit -m "refactor: remove StackCommand::New (replaced by OpenCommand::InNewStack) (VMX-124)"
```

---

## Task 17: Remove `TabCommand::New` (its work now lives in `InNewTab` handler)

**Files:**
- Modify: `crates/vmux_command/src/command.rs`
- Modify: `crates/vmux_layout/src/space.rs:181` (drop `TabCommand::New` arm)
- Modify: any call site that emitted `TabCommand::New` — replace with `OpenCommand::InNewTab { url: None }`

- [ ] **Step 1: Write the failing test**

In `command.rs`:

```rust
#[test]
fn new_tab_id_no_longer_resolves() {
    assert!(AppCommand::from_menu_id("new_tab").is_none());
}
```

- [ ] **Step 2: Run the test (should fail)**

Run: `bash -c "cargo test -p vmux_command --lib tests::new_tab_id_no_longer_resolves 2>&1 | tail -10"`
Expected: assertion failure.

- [ ] **Step 3: Remove the variant**

Drop `#[menu(id = "new_tab", ...)] New` from `TabCommand`. Update `#[default]`.

In `space.rs:181`, drop the `TabCommand::New => { ... }` arm.

Grep for callers:

```bash
bash -c "rg -n 'TabCommand::New' crates/"
```

Replace each.

- [ ] **Step 4: Run tests + lint**

Run: `bash -c "cargo fmt -p vmux_command -p vmux_layout -- --check && env -u CEF_PATH cargo clippy -p vmux_command -p vmux_layout --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_command -p vmux_layout 2>&1 | tail -30"`
Expected: green.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_command/src/command.rs crates/vmux_layout/src/space.rs
git commit -m "refactor: remove TabCommand::New (replaced by OpenCommand::InNewTab) (VMX-124)"
```

---

## Task 18: Remove `PaneCommand::SplitV` / `SplitH` (work moved to `InPane` handler + chord aliases)

**Files:**
- Modify: `crates/vmux_command/src/command.rs`
- Modify: `crates/vmux_layout/src/pane.rs:514-520` (drop arm)
- Modify: any call site emitting `PaneCommand::SplitV/H`

- [ ] **Step 1: Write the failing test**

In `command.rs`:

```rust
#[test]
fn split_v_and_split_h_no_longer_resolve() {
    assert!(AppCommand::from_menu_id("split_v").is_none());
    assert!(AppCommand::from_menu_id("split_h").is_none());
}

#[test]
fn tmux_chord_percent_resolves_to_in_pane_right_new_split() {
    use vmux_command::open::{OpenCommand, PaneDirection, PaneTarget, PaneOpenMode};
    let extras = OpenCommand::extra_chord_bindings();
    let found = extras.iter().any(|(_, variant)| matches!(
        variant,
        OpenCommand::InPane {
            direction: PaneDirection::Right,
            target: PaneTarget::NewSplit,
            mode: PaneOpenMode::NewStack,
            url: None,
        }
    ));
    assert!(found, "Ctrl+g, % should bind to InPane Right NewSplit NewStack");
}
```

- [ ] **Step 2: Run the tests (should fail)**

Run: `bash -c "cargo test -p vmux_command --lib tests::split_v_and_split_h_no_longer_resolve tests::tmux_chord_percent_resolves_to_in_pane_right_new_split 2>&1 | tail -20"`
Expected: assertion failures.

- [ ] **Step 3: Remove variants**

Drop `SplitV` and `SplitH` from `PaneCommand` in `command.rs`. Update `#[default]`.

In `pane.rs:514-520`, drop the `PaneCommand::SplitV | PaneCommand::SplitH => { ... }` arm. The split logic now lives in Task 11's `handle_open_in_pane_command`.

- [ ] **Step 4: Run tests + lint**

Run: `bash -c "cargo fmt -p vmux_command -p vmux_layout -- --check && env -u CEF_PATH cargo clippy -p vmux_command -p vmux_layout --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_command -p vmux_layout 2>&1 | tail -30"`
Expected: green, including the new chord-binding assertion.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_command/src/command.rs crates/vmux_layout/src/pane.rs
git commit -m "refactor: remove PaneCommand::SplitV/SplitH (replaced by OpenCommand::InPane + tmux chord aliases) (VMX-124)"
```

---

## Task 19: Remove `BrowserCommand::FocusAddressBar` (super+l now binds InPlace)

**Files:**
- Modify: `crates/vmux_command/src/command.rs` (already removed in Task 8's BrowserCommand restructure, verify)
- Modify: `crates/vmux_desktop/src/command_bar.rs` and any other site referencing `BrowserCommand::FocusAddressBar`

- [ ] **Step 1: Write the failing test**

```rust
// In crates/vmux_command/src/command.rs tests
#[test]
fn browser_focus_address_bar_id_gone() {
    assert!(AppCommand::from_menu_id("browser_focus_address_bar").is_none());
}

#[test]
fn super_l_now_binds_open_in_place() {
    // super+l is `accel = "super+l"` on OpenCommand::InPlace per Task 6.
    // Confirm at the shortcut table level.
    let bindings = AppCommand::default_shortcuts();
    let bound = bindings.iter().find(|(_, id)| id == "open_in_place");
    assert!(bound.is_some(), "open_in_place should appear in default_shortcuts");
}
```

- [ ] **Step 2: Run the tests (should fail)**

Run: `bash -c "cargo test -p vmux_command --lib tests 2>&1 | tail -30"`
Expected: `browser_focus_address_bar_id_gone` may already pass if Task 8 removed it; `super_l_now_binds_open_in_place` may pass too. If both already pass, this task collapses to call-site cleanup only.

- [ ] **Step 3: Remove call sites**

```bash
bash -c "rg -n 'FocusAddressBar' crates/"
```

For each hit, replace `BrowserCommand::FocusAddressBar` with `AppCommand::Browser(BrowserCommand::Open(OpenCommand::InPlace { url: None }))` (or `OpenCommand::InPlace { url: None }` if the surrounding type is already specific).

- [ ] **Step 4: Run tests + lint**

Run: `bash -c "cargo fmt -p vmux_command -p vmux_desktop -p vmux_layout -- --check && env -u CEF_PATH cargo clippy -p vmux_command -p vmux_desktop -p vmux_layout --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_command -p vmux_desktop -p vmux_layout 2>&1 | tail -30"`
Expected: green.

- [ ] **Step 5: Commit**

```bash
git add -u
git commit -m "refactor: drop BrowserCommand::FocusAddressBar (super+l binds OpenCommand::InPlace) (VMX-124)"
```

---

## Task 20: Re-route `TerminalCommand::New` / `NewTab` through `OpenCommand`

**Files:**
- Modify: `crates/vmux_command/src/command.rs` (drop `TerminalCommand::New` and `NewTab` variants)
- Modify: `crates/vmux_terminal/src/plugin.rs` (callers that previously emitted these commands now emit `OpenCommand::InNewStack` or `InNewTab` with `vmux://terminal/` URL)
- Modify: `crates/vmux_desktop/src/shortcut.rs` (rebind `ctrl+\``)

- [ ] **Step 1: Write the failing test**

```rust
// In crates/vmux_command/src/command.rs tests
#[test]
fn terminal_new_and_new_tab_ids_gone() {
    assert!(AppCommand::from_menu_id("terminal_new").is_none());
    assert!(AppCommand::from_menu_id("terminal_new_tab").is_none());
}
```

- [ ] **Step 2: Run the test (should fail)**

Run: `bash -c "cargo test -p vmux_command --lib tests::terminal_new_and_new_tab_ids_gone 2>&1 | tail -10"`
Expected: assertion failure.

- [ ] **Step 3: Drop variants + rebind callers**

In `command.rs`, drop `TerminalCommand::New` and `TerminalCommand::NewTab`. Update `#[default]`.

In `crates/vmux_terminal/src/plugin.rs`, find systems that handle `TerminalCommand::New` / `NewTab` and either remove them or convert them into emitters of:

```rust
messages.write(AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
    url: Some("vmux://terminal/".to_string()),
})));
```

For `NewTab` variant (`ctrl+\``), the synthesised command is `OpenCommand::InNewTab { url: Some("vmux://terminal/") }`. But since the shortcut was bound to `TerminalCommand::NewTab` via the derive, we need a direct shortcut registration for `ctrl+\``. Add to `crates/vmux_desktop/src/shortcut.rs` an explicit binding mapping `Ctrl+\`` to `AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack { url: Some("vmux://terminal/".into()) }))`.

(Note: `ctrl+\`` originally meant "new terminal in new tab" — but the new semantics map it to a new in-pane stack containing a terminal, matching the "open a terminal next to me" UX. If the user wants a fresh workspace Tab, use `super+t` then a terminal URL; the spec assumes the in-pane mapping is correct. If feedback says otherwise, switch to `OpenCommand::InNewTab`.)

- [ ] **Step 4: Run tests + lint**

Run: `bash -c "cargo fmt -p vmux_command -p vmux_terminal -p vmux_desktop -- --check && env -u CEF_PATH cargo clippy -p vmux_command -p vmux_terminal -p vmux_desktop --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_command -p vmux_terminal -p vmux_desktop 2>&1 | tail -30"`
Expected: green.

- [ ] **Step 5: Commit**

```bash
git add -u
git commit -m "refactor(terminal): TerminalCommand::New/NewTab route through OpenCommand (VMX-124)"
```

---

## Task 21: Update `vmux_history` and `vmux_agent` to emit `OpenCommand::InNewStack`

**Files:**
- Modify: `crates/vmux_history/src/plugin.rs` (existing `OpenInNewStack` history-click flow)
- Modify: `crates/vmux_agent/src/plugin.rs` (existing agent `OpenInNewStack` event)

- [ ] **Step 1: Search for existing routes**

```bash
bash -c "rg -n 'OpenInNewStack|HistoryOpenIntent' crates/vmux_history crates/vmux_agent crates/vmux_core"
```

For each handler that today wires the click/intent into a stack-creation path, change the terminal action to:

```rust
messages.write(AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
    url: Some(target_url),
})));
```

- [ ] **Step 2: Write a regression test in each crate**

For `vmux_history`:

```rust
// crates/vmux_history/tests/history_click_emits_open_command.rs
// Build App. Spawn a history Visit at "https://x". Trigger HistoryOpenIntent
// for that visit. Assert AppCommand::Browser(BrowserCommand::Open(InNewStack { url: Some("https://x") }))
// queued.
```

For `vmux_agent`, mirror the structure if there's an agent test fixture; otherwise note that the change is exercised by integration tests in `vmux_layout`.

- [ ] **Step 3: Run the tests and verify they fail before changing the emitter**

Run: `bash -c "cargo test -p vmux_history 2>&1 | tail -20"`
Expected: failure.

- [ ] **Step 4: Implement the change in both crates**

(Update each emitter as described in Step 1.)

- [ ] **Step 5: Run tests + lint on each crate**

Run: `bash -c "cargo fmt -p vmux_history -p vmux_agent -- --check && env -u CEF_PATH cargo clippy -p vmux_history -p vmux_agent --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_history -p vmux_agent 2>&1 | tail -30"`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_history crates/vmux_agent
git commit -m "refactor(history,agent): emit OpenCommand::InNewStack for open-url intents (VMX-124)"
```

---

## Task 22: Persisted-session migration for renamed IPC fields

**Files:**
- Modify: `crates/vmux_layout/src/snapshot.rs` (or wherever rkyv-backed session files are read) — bump schema version + add migration arm for old `new_tab: bool` payloads
- Test: `crates/vmux_layout/tests/session_migration.rs`

- [ ] **Step 1: Survey existing snapshot path**

```bash
bash -c "rg -n 'CommandBarOpenEvent|rkyv|deserialize' crates/vmux_layout/src/snapshot.rs | head -30"
```

Identify whether old persisted snapshots actually contain `CommandBarOpenEvent` (likely no — it's a transient webview message). If only the live IPC is affected, no migration is required and this task is documentation only.

- [ ] **Step 2: If migration needed, write the failing test**

```rust
// crates/vmux_layout/tests/session_migration.rs
// Synthesise old-format bytes (rkyv-encoded { new_tab: true } shape).
// Deserialise via the migration path. Assert it lands as
// CommandBarOpenEvent { target: Some(OpenTarget::InNewStack), .. }.
```

- [ ] **Step 3: If no persisted snapshots reference the event, document under spec open-issue 4 (already noted) and skip migration**

Update `docs/specs/2026-05-22-open-command-consolidation-design.md` open-issue 2 (IPC breaking changes) to record the verified scope: "Snapshots do not persist CommandBarOpenEvent; only the live webview IPC contract changes, no migration code needed." Or implement the migration and remove the open issue.

- [ ] **Step 4: Commit (whichever path taken)**

```bash
git add -u
git commit -m "chore: document/implement session migration for OpenTarget IPC (VMX-124)"
```

---

## Task 23: End-to-end integration smoke test

**Files:**
- Create: `crates/vmux_layout/tests/open_command_smoke.rs`

- [ ] **Step 1: Write the smoke test**

```rust
// crates/vmux_layout/tests/open_command_smoke.rs
// Build full Bevy App with LayoutPlugin + StackPlugin + PanePlugin + SpacePlugin
// + ProfilePlugin. Walk through:
//   1) Default state: one Profile, one Tab (Space), one Pane, one Stack at startup URL.
//   2) Emit InNewStack { url: Some("https://a") } -> assert 2 stacks now.
//   3) Emit InPane { Right, NewSplit, NewStack, Some("https://b") } -> assert split tree has a right pane with a stack at b.
//   4) Emit InNewTab { url: Some("https://c") } -> assert second Tab spawned.
//   5) Emit InNewSpace { url: Some("https://d") } -> assert second Profile spawned.
//   6) Emit InPlace { url: Some("https://e") } -> assert focused stack now at e.
```

- [ ] **Step 2: Run the test and verify it passes**

Run: `bash -c "cargo test -p vmux_layout --test open_command_smoke 2>&1 | tail -40"`
Expected: all 6 phases pass.

- [ ] **Step 3: Run full changed-crate check**

Run: `bash -c "PKGS=\$(BASE=origin/main ./scripts/changed-crates.sh); for pkg in \$PKGS; do echo '=== '\$pkg; cargo fmt -p \$pkg -- --check; env -u CEF_PATH cargo clippy -p \$pkg --all-targets -- -D warnings; env -u CEF_PATH cargo test -p \$pkg; done 2>&1 | tail -60"`
Expected: green across every changed crate.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_layout/tests/open_command_smoke.rs
git commit -m "test(vmux_layout): OpenCommand end-to-end smoke (VMX-124)"
```

---

## Task 24: MCP description audit + tool-name verification

**Files:**
- Modify: `crates/vmux_mcp/tests/mcp_smoke.rs` (extend with new tool assertions) — verify the open_in_* tools exist with the expected descriptions

- [ ] **Step 1: Write the failing test**

```rust
// crates/vmux_mcp/tests/mcp_smoke.rs (extend, do not replace existing tests)
#[test]
fn open_command_tools_exposed() {
    let entries = vmux_command::AppCommand::mcp_tool_entries();
    let ids: Vec<_> = entries.iter().map(|(id, _, _)| id.as_str()).collect();
    assert!(ids.contains(&"open_in_place"));
    assert!(ids.contains(&"open_in_new_stack"));
    assert!(ids.contains(&"open_in_pane_top"));
    assert!(ids.contains(&"open_in_pane_right"));
    assert!(ids.contains(&"open_in_pane_bottom"));
    assert!(ids.contains(&"open_in_pane_left"));
    assert!(ids.contains(&"open_in_new_tab"));
    assert!(ids.contains(&"open_in_new_space"));
}

#[test]
fn open_in_pane_tool_schema_enum_constraints() {
    let entries = vmux_command::AppCommand::mcp_tool_entries();
    let (_, _, schema) = entries
        .iter()
        .find(|(id, _, _)| id == "open_in_pane_right")
        .expect("open_in_pane_right tool");
    let props = schema.get("properties").expect("schema has properties");
    let target = props.get("target").expect("schema has target field");
    let target_enum = target.get("enum").expect("target has enum constraint");
    assert!(target_enum.as_array().unwrap().iter().any(|v| v == "existing"));
    assert!(target_enum.as_array().unwrap().iter().any(|v| v == "new_split"));
}
```

- [ ] **Step 2: Run the tests and verify they fail until the expand macro produces tool names**

Run: `bash -c "cargo test -p vmux_mcp 2>&1 | tail -30"`
Expected: failures if Task 5 hasn't produced the expanded tool names.

- [ ] **Step 3: If failures point to missing expand output for McpTool**

The original spec's `McpTool` already supports `Fields::Named` but doesn't fan out the `expand` attribute. Extend the `McpTool` derive in `crates/vmux_macro/src/lib.rs` (`impl_mcp_tool_leaf_fielded`) to honour `expand = "direction"` and emit four distinct tools (`open_in_pane_top` etc.).

- [ ] **Step 4: Run tests + lint**

Run: `bash -c "cargo fmt -p vmux_macro -p vmux_mcp -- --check && env -u CEF_PATH cargo clippy -p vmux_macro -p vmux_mcp --all-targets -- -D warnings && env -u CEF_PATH cargo test -p vmux_macro -p vmux_mcp 2>&1 | tail -30"`
Expected: green.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_macro/src/lib.rs crates/vmux_mcp/tests/mcp_smoke.rs
git commit -m "feat(vmux_macro,vmux_mcp): McpTool honours #[menu(expand)] for OpenCommand::InPane (VMX-124)"
```

---

## Task 25: Delete plan file + final cleanup

**Files:**
- Delete: `docs/plans/2026-05-22-open-command-consolidation.md` (per project rule: delete plan once implemented)

- [ ] **Step 1: Run full changed-crate validation**

Run: `bash -c "PKGS=\$(BASE=origin/main ./scripts/changed-crates.sh); for pkg in \$PKGS; do cargo fmt -p \$pkg -- --check && env -u CEF_PATH cargo clippy -p \$pkg --all-targets -- -D warnings && env -u CEF_PATH cargo test -p \$pkg; done 2>&1 | tail -20"`
Expected: green.

- [ ] **Step 2: Delete the plan file**

```bash
git rm docs/plans/2026-05-22-open-command-consolidation.md
```

- [ ] **Step 3: Commit**

```bash
git commit -m "chore: remove implemented plan (VMX-124)"
```

- [ ] **Step 4: Push branch**

```bash
git push -u origin vmx-124-consolidate-page-open-commands
```

- [ ] **Step 5: Create PR**

Use the `open-new-pr` skill or `gh pr create` to open a PR titled "VMX-124 Consolidate open commands under BrowserCommand::Open" with a body summarising the spec sections.
