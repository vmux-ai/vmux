# Page Strategy ECS Migration Plan

> **For agentic workers:** Use superpowers:subagent-driven-development or superpowers:executing-plans to implement task-by-task. Steps use `- [ ]` checkboxes.

**Goal:** Replace `AgentPageStrategy` trait + `HashMap<(String,String), Arc<dyn AgentPageStrategy>>` with one Entity per registered Page strategy, queryable via Bevy ECS.

**Architecture:**
- Per-strategy Entity carries metadata as components (`StrategyKey`, `Endpoint`, `EnvVar`, `AgentKind`, `AgentVariant`, provider marker) and hot-path callables as fn-pointer components (`BuildRequestFn`, `ParseSseFn`).
- A `PageStrategyIndex` resource maintains `(provider, model) → Entity` for O(1) lookup; indexer observers keep it in sync.
- Hot path (`drive_sse`) takes a raw `fn(&str) -> Option<StreamEvent>` instead of `Arc<dyn AgentPageStrategy>`, preserving zero-cost dispatch.
- Per-provider behavior lives in free functions inside `providers::{mistral,anthropic,openai,echo}`; their plugins spawn the entity when the env var is present.
- CLI strategies stay trait-based for now (out of scope).

**Tech Stack:** Bevy 0.18 ECS, reqwest, tokio, crossbeam_channel.

**Migration is staged:**
- Phase 1: add ECS scaffolding alongside the trait (no behavior change).
- Phase 2: migrate `drive_sse` to fn-pointer (still produced by the trait).
- Phase 3: pilot Mistral end-to-end via ECS, dropping its trait impl.
- Phase 4: migrate Anthropic, OpenAI, Echo.
- Phase 5: delete trait + map + consumers' trait dependency.
- Phase 6 (optional follow-up): add `ErrorCount`, `LastUsedAt`, `RateLimit` components.

Each phase compiles and passes tests independently — safe to land in separate PRs if desired.

---

## Phase 1: ECS Scaffolding

### Task 1.1: Component module

**Files:**
- Create: `crates/vmux_agent/src/client/page/strategy_components.rs`
- Modify: `crates/vmux_agent/src/client/page.rs` (or wherever `page` mod is declared) — add `pub mod strategy_components;`

- [ ] **Step 1: Write failing test**

Create `crates/vmux_agent/src/client/page/strategy_components.rs`:

```rust
use bevy::prelude::*;

use crate::message::Message;
use crate::stream::{StreamEvent, ToolDef};
use crate::{AgentKind, AgentVariant};

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Strategy;

#[derive(Component, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StrategyKey {
    pub provider: String,
    pub model: String,
}

#[derive(Component, Debug, Clone)]
pub struct Endpoint(pub String);

#[derive(Component, Debug, Clone, Copy)]
pub struct EnvVarName(pub &'static str);

#[derive(Component, Debug, Clone, Copy)]
pub struct StrategyKind(pub AgentKind);

#[derive(Component, Debug, Clone, Copy)]
pub struct StrategyVariant(pub AgentVariant);

pub type BuildRequest =
    fn(model: &str, messages: &[Message], tools: &[ToolDef], api_key: &str) -> reqwest::Request;

pub type ParseSse = fn(payload: &str) -> Option<StreamEvent>;

#[derive(Component, Clone, Copy)]
pub struct BuildRequestFn(pub BuildRequest);

#[derive(Component, Clone, Copy)]
pub struct ParseSseFn(pub ParseSse);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategy_key_equality_is_provider_then_model() {
        let a = StrategyKey {
            provider: "mistral".into(),
            model: "devstral-2".into(),
        };
        let b = StrategyKey {
            provider: "mistral".into(),
            model: "devstral-2".into(),
        };
        let c = StrategyKey {
            provider: "mistral".into(),
            model: "other".into(),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
```

- [ ] **Step 2: Wire module**

Find the page module declaration and add a sibling. Likely in `crates/vmux_agent/src/client/page.rs`:

```rust
pub mod strategy_components;
```

- [ ] **Step 3: Run tests**

```
cargo test -p vmux_agent strategy_components::
```
Expected: 1 test passes.

- [ ] **Step 4: Commit**

```
git add crates/vmux_agent/src/client/page/strategy_components.rs crates/vmux_agent/src/client/page.rs
git commit -m "feat(vmux_agent): introduce Page strategy ECS components"
```

---

### Task 1.2: Strategy index resource

**Files:**
- Create: `crates/vmux_agent/src/client/page/strategy_index.rs`
- Modify: `crates/vmux_agent/src/client/page.rs` — add `pub mod strategy_index;`

- [ ] **Step 1: Write failing test**

`crates/vmux_agent/src/client/page/strategy_index.rs`:

```rust
use std::collections::HashMap;

use bevy::prelude::*;

#[derive(Resource, Default, Debug)]
pub struct PageStrategyIndex {
    by_key: HashMap<(String, String), Entity>,
}

impl PageStrategyIndex {
    pub fn insert(&mut self, provider: &str, model: &str, entity: Entity) {
        self.by_key
            .insert((provider.to_string(), model.to_string()), entity);
    }

    pub fn remove(&mut self, provider: &str, model: &str) -> Option<Entity> {
        self.by_key.remove(&(provider.to_string(), model.to_string()))
    }

    pub fn get(&self, provider: &str, model: &str) -> Option<Entity> {
        self.by_key
            .get(&(provider.to_string(), model.to_string()))
            .copied()
    }

    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove_round_trip() {
        let mut idx = PageStrategyIndex::default();
        let e = Entity::from_raw(42);
        idx.insert("mistral", "devstral-2", e);
        assert_eq!(idx.get("mistral", "devstral-2"), Some(e));
        assert_eq!(idx.remove("mistral", "devstral-2"), Some(e));
        assert_eq!(idx.get("mistral", "devstral-2"), None);
    }
}
```

- [ ] **Step 2: Run tests**

```
cargo test -p vmux_agent strategy_index::
```
Expected: 1 test passes.

- [ ] **Step 3: Commit**

```
git add crates/vmux_agent/src/client/page/strategy_index.rs crates/vmux_agent/src/client/page.rs
git commit -m "feat(vmux_agent): add PageStrategyIndex resource"
```

---

### Task 1.3: Indexer observers

**Files:**
- Create: `crates/vmux_agent/src/client/page/strategy_indexer.rs`
- Modify: `crates/vmux_agent/src/client/page.rs` — add `pub mod strategy_indexer;`
- Modify: `crates/vmux_agent/src/client/page/plugin.rs` — register resource + observers

- [ ] **Step 1: Write failing test**

`crates/vmux_agent/src/client/page/strategy_indexer.rs`:

```rust
use bevy::prelude::*;

use crate::client::page::strategy_components::StrategyKey;
use crate::client::page::strategy_index::PageStrategyIndex;

pub fn on_strategy_added(
    trigger: On<Add, StrategyKey>,
    keys: Query<&StrategyKey>,
    mut idx: ResMut<PageStrategyIndex>,
) {
    let e = trigger.entity;
    let Ok(key) = keys.get(e) else {
        return;
    };
    idx.insert(&key.provider, &key.model, e);
}

pub fn on_strategy_removed(
    trigger: On<Remove, StrategyKey>,
    keys: Query<&StrategyKey>,
    mut idx: ResMut<PageStrategyIndex>,
) {
    let e = trigger.entity;
    let Ok(key) = keys.get(e) else {
        return;
    };
    idx.remove(&key.provider, &key.model);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::page::strategy_components::{
        EnvVarName, Strategy, StrategyKey, StrategyKind, StrategyVariant,
    };
    use crate::{AgentKind, AgentVariant};

    fn test_app() -> App {
        let mut app = App::new();
        app.insert_resource(PageStrategyIndex::default());
        app.add_observer(on_strategy_added);
        app.add_observer(on_strategy_removed);
        app
    }

    fn strategy_bundle(provider: &str, model: &str) -> impl Bundle {
        (
            Strategy,
            StrategyKey {
                provider: provider.into(),
                model: model.into(),
            },
            EnvVarName("FAKE"),
            StrategyKind(AgentKind::Vibe),
            StrategyVariant(AgentVariant::Page),
        )
    }

    #[test]
    fn spawn_inserts_into_index() {
        let mut app = test_app();
        let e = app.world_mut().spawn(strategy_bundle("p", "m")).id();
        app.update();
        let idx = app.world().resource::<PageStrategyIndex>();
        assert_eq!(idx.get("p", "m"), Some(e));
    }

    #[test]
    fn despawn_removes_from_index() {
        let mut app = test_app();
        let e = app.world_mut().spawn(strategy_bundle("p", "m")).id();
        app.update();
        app.world_mut().entity_mut(e).despawn();
        app.update();
        let idx = app.world().resource::<PageStrategyIndex>();
        assert!(idx.get("p", "m").is_none());
    }
}
```

> Verify `On<Add, T>` / `On<Remove, T>` API matches the bevy version in use (`cargo tree -p vmux_agent | grep '^bevy '`). If signature differs, adapt observer signatures before committing.

- [ ] **Step 2: Run tests**

```
cargo test -p vmux_agent strategy_indexer::
```
Expected: both tests pass.

- [ ] **Step 3: Wire into PageAgentPlugin**

Modify `crates/vmux_agent/src/client/page/plugin.rs` inside `impl Plugin for PageAgentPlugin { fn build(&self, app: &mut App) { ... } }`:

Add near the existing resource init:

```rust
if app.world().get_resource::<crate::client::page::strategy_index::PageStrategyIndex>().is_none() {
    app.insert_resource(crate::client::page::strategy_index::PageStrategyIndex::default());
}
app.add_observer(crate::client::page::strategy_indexer::on_strategy_added);
app.add_observer(crate::client::page::strategy_indexer::on_strategy_removed);
```

- [ ] **Step 4: Run full agent test suite**

```
env -u CEF_PATH cargo test -p vmux_agent
```
Expected: previously passing tests still pass.

- [ ] **Step 5: Commit**

```
git add crates/vmux_agent/src/client/page/
git commit -m "feat(vmux_agent): index Page strategy entities by (provider, model)"
```

---

## Phase 2: Migrate Hot Path to fn-pointer

### Task 2.1: drive_sse takes a parser fn pointer

**Files:**
- Modify: `crates/vmux_agent/src/http.rs`

- [ ] **Step 1: Replace signature**

Find the `drive_sse` function. Replace:

```rust
use crate::client::page::strategy::AgentPageStrategy;

pub async fn drive_sse(
    request: reqwest::Request,
    strategy: Arc<dyn AgentPageStrategy>,
    tx: Sender<StreamEvent>,
) {
```

with:

```rust
use crate::client::page::strategy_components::ParseSse;

pub async fn drive_sse(
    request: reqwest::Request,
    parse_sse: ParseSse,
    tx: Sender<StreamEvent>,
) {
```

Inside the loop, replace `strategy.parse_sse_event(frame)` with `parse_sse(frame)`.

Remove the now-unused `use std::sync::Arc;` if it becomes dead.

- [ ] **Step 2: Update http.rs tests**

In `mod tests`, drop the `EchoTextStrategy` struct + impls. Replace with a free fn and rewrite the two tests to pass it:

```rust
fn echo_parse(payload: &str) -> Option<StreamEvent> {
    payload
        .strip_prefix("data: ")
        .map(|s| StreamEvent::TextDelta(s.to_string()))
}

// In each test:
drive_sse(req, echo_parse, tx).await;
```

Delete the `AgentStrategy` / `AgentPageStrategy` imports and impls inside `tests`.

- [ ] **Step 3: Update all callers of `drive_sse`**

```
grep -rn 'drive_sse(' crates/
```

For each caller (likely `systems/process_input.rs` and `systems/continue_after_tool.rs`), change `Arc::clone(&strategy)` (or similar) to a `ParseSseFn` lookup. For now, since strategies still implement the trait, define a temporary adapter:

In `crates/vmux_agent/src/http.rs` (or a small new file), add:

```rust
pub fn parse_sse_via_strategy<S: crate::client::page::strategy::AgentPageStrategy + 'static>(
    s: &S,
) -> ParseSse {
    fn forward<S: crate::client::page::strategy::AgentPageStrategy + 'static>(_p: &str) -> Option<StreamEvent> {
        unreachable!("placeholder; replaced per-provider in Phase 3")
    }
    forward::<S>
}
```

> Note: this adapter is a stopgap. The real callers will instead read `ParseSseFn` from the entity's components in Phase 3+. To avoid a half-broken state, we keep the trait alive AND add a `parse_sse_fn(&self) -> ParseSse` method on `AgentPageStrategy` so each impl returns its provider's free function. See Task 2.2.

- [ ] **Step 4: Skip commit until Task 2.2 lands**

This file changes in lockstep with the next task. Don't commit yet.

---

### Task 2.2: Trait exposes ParseSse fn pointer

**Files:**
- Modify: `crates/vmux_agent/src/client/page/strategy.rs`
- Modify: `crates/vmux_agent/src/providers/{mistral,anthropic,openai}.rs`
- Modify: `crates/vmux_agent/src/echo.rs`
- Modify: `crates/vmux_agent/src/systems/process_input.rs`
- Modify: `crates/vmux_agent/src/systems/continue_after_tool.rs`

- [ ] **Step 1: Add method to trait**

In `crates/vmux_agent/src/client/page/strategy.rs`:

```rust
use crate::client::page::strategy_components::ParseSse;

pub trait AgentPageStrategy: AgentStrategy {
    fn provider(&self) -> &str;
    fn model(&self) -> &str;
    fn endpoint(&self) -> &str;
    fn env_var(&self) -> &'static str;

    fn build_request(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolDef],
        api_key: &str,
    ) -> reqwest::Request;

    fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent>;

    fn parse_sse_fn(&self) -> ParseSse;
}
```

- [ ] **Step 2: Implement on each provider**

Define a free `pub fn parse_sse(payload: &str) -> Option<StreamEvent>` in each provider module that re-uses the existing parser body (currently inside `parse_sse_event`), then point both at it:

Example for `crates/vmux_agent/src/providers/mistral.rs`:

```rust
pub fn parse_sse(payload: &str) -> Option<StreamEvent> {
    crate::providers::openai_shared::parse_chat_completions_sse(payload)
}

impl AgentPageStrategy for MistralStrategy {
    // ... existing methods unchanged ...
    fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent> {
        parse_sse(payload)
    }
    fn parse_sse_fn(&self) -> ParseSse {
        parse_sse
    }
}
```

Repeat the same shape for `anthropic.rs`, `openai.rs`, and `echo.rs` — each module gets a `pub fn parse_sse` returning what the impl used to return.

- [ ] **Step 3: Replace `drive_sse` call sites**

In `crates/vmux_agent/src/systems/process_input.rs` and `crates/vmux_agent/src/systems/continue_after_tool.rs`, find where `drive_sse(req, Arc::clone(&strategy), tx)` (or similar) is invoked. Change to:

```rust
let parse_sse = strategy.parse_sse_fn();
// existing tokio spawn
tokio::spawn(crate::http::drive_sse(request, parse_sse, tx));
```

(Or whatever runtime spawner is in place — match the existing pattern.)

- [ ] **Step 4: Delete the Phase 2.1 stopgap adapter**

Remove `parse_sse_via_strategy` from `crates/vmux_agent/src/http.rs` if it was added.

- [ ] **Step 5: Build + test**

```
cargo fmt -p vmux_agent
env -u CEF_PATH cargo clippy -p vmux_agent --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_agent
```
Expected: all green.

- [ ] **Step 6: Commit**

```
git add crates/vmux_agent/
git commit -m "refactor(vmux_agent): drive_sse takes ParseSse fn pointer

Each provider exposes a free parse_sse fn; AgentPageStrategy::parse_sse_fn
returns it. drive_sse no longer needs the trait object — preparation for
ECS migration of the strategy registry."
```

---

## Phase 3: Pilot — Mistral via ECS

### Task 3.1: Mistral build_request as free fn

**Files:**
- Modify: `crates/vmux_agent/src/providers/mistral.rs`

- [ ] **Step 1: Extract free function**

Move the body of `MistralStrategy::build_request` into:

```rust
pub fn build_request(
    model: &str,
    messages: &[crate::message::Message],
    tools: &[crate::stream::ToolDef],
    api_key: &str,
) -> reqwest::Request {
    // ... existing body, with `self.endpoint()` replaced by the literal endpoint constant ...
}

pub const ENDPOINT: &str = "https://api.mistral.ai/v1/chat/completions";
pub const ENV_VAR: &str = "MISTRAL_API_KEY";
pub const PROVIDER: &str = "mistral";

impl AgentPageStrategy for MistralStrategy {
    // ...
    fn build_request(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolDef],
        api_key: &str,
    ) -> reqwest::Request {
        build_request(model, messages, tools, api_key)
    }
    fn endpoint(&self) -> &str {
        ENDPOINT
    }
    fn env_var(&self) -> &'static str {
        ENV_VAR
    }
    fn provider(&self) -> &str {
        PROVIDER
    }
    // ...
}
```

- [ ] **Step 2: Verify existing tests still pass**

```
env -u CEF_PATH cargo test -p vmux_agent providers::mistral::
```
Expected: pass.

- [ ] **Step 3: Commit**

```
git add crates/vmux_agent/src/providers/mistral.rs
git commit -m "refactor(vmux_agent): extract Mistral build_request as free fn"
```

---

### Task 3.2: Mistral plugin spawns Strategy entity

**Files:**
- Create: `crates/vmux_agent/src/providers/mistral_plugin.rs`
- Modify: `crates/vmux_agent/src/providers.rs` — `pub mod mistral_plugin;`
- Modify: `crates/vmux_agent/src/client/page/plugin.rs` — add `MistralPlugin` to `PageAgentPlugin`

- [ ] **Step 1: Write test**

`crates/vmux_agent/src/providers/mistral_plugin.rs`:

```rust
use bevy::prelude::*;

use crate::AgentKind;
use crate::AgentVariant;
use crate::client::page::strategy_components::{
    BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
    StrategyVariant,
};
use crate::client::page::strategy_index::PageStrategyIndex;

#[derive(Component, Debug, Clone, Copy)]
pub struct MistralProvider;

pub struct MistralPlugin;

impl Plugin for MistralPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            register_mistral_strategy.after(vmux_setting::SettingsLoadSet),
        );
    }
}

fn register_mistral_strategy(
    mut commands: Commands,
    idx: Option<Res<PageStrategyIndex>>,
) {
    if std::env::var(super::mistral::ENV_VAR).is_err() {
        return;
    }
    let key = StrategyKey {
        provider: super::mistral::PROVIDER.to_string(),
        model: "devstral-2".to_string(),
    };
    if let Some(idx) = idx.as_deref()
        && idx.get(&key.provider, &key.model).is_some()
    {
        return;
    }
    commands.spawn((
        Strategy,
        MistralProvider,
        key,
        Endpoint(super::mistral::ENDPOINT.to_string()),
        EnvVarName(super::mistral::ENV_VAR),
        StrategyKind(AgentKind::Vibe),
        StrategyVariant(AgentVariant::Page),
        BuildRequestFn(super::mistral::build_request),
        ParseSseFn(super::mistral::parse_sse),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::page::strategy_indexer::{on_strategy_added, on_strategy_removed};
    use serial_test::serial;

    fn test_app() -> App {
        let mut app = App::new();
        app.insert_resource(PageStrategyIndex::default());
        app.add_observer(on_strategy_added);
        app.add_observer(on_strategy_removed);
        app.add_plugins(MistralPlugin);
        app
    }

    #[test]
    #[serial]
    fn spawns_entity_when_env_var_set() {
        unsafe { std::env::set_var(super::super::mistral::ENV_VAR, "x") };
        let mut app = test_app();
        app.update();
        let idx = app.world().resource::<PageStrategyIndex>();
        assert!(idx.get("mistral", "devstral-2").is_some());
        unsafe { std::env::remove_var(super::super::mistral::ENV_VAR) };
    }

    #[test]
    #[serial]
    fn does_not_spawn_without_env_var() {
        unsafe { std::env::remove_var(super::super::mistral::ENV_VAR) };
        let mut app = test_app();
        app.update();
        let idx = app.world().resource::<PageStrategyIndex>();
        assert!(idx.get("mistral", "devstral-2").is_none());
    }
}
```

- [ ] **Step 2: Wire module**

`crates/vmux_agent/src/providers.rs`:

```rust
pub mod mistral_plugin;
```

`crates/vmux_agent/src/client/page/plugin.rs` — inside `PageAgentPlugin::build`:

```rust
app.add_plugins(crate::providers::mistral_plugin::MistralPlugin);
```

- [ ] **Step 3: Run tests**

```
env -u CEF_PATH cargo test -p vmux_agent providers::mistral_plugin::
```
Expected: both tests pass.

- [ ] **Step 4: Commit**

```
git add crates/vmux_agent/src/providers/ crates/vmux_agent/src/client/page/plugin.rs
git commit -m "feat(vmux_agent): MistralPlugin spawns Strategy entity on startup"
```

---

### Task 3.3: Consumers read fn pointers from entity

**Files:**
- Modify: `crates/vmux_agent/src/systems/process_input.rs`
- Modify: `crates/vmux_agent/src/systems/continue_after_tool.rs`

- [ ] **Step 1: Find existing lookups**

```
grep -n 'get_page_by_provider_model' crates/vmux_agent/src/systems/
```

- [ ] **Step 2: Add ECS lookup helper**

In `crates/vmux_agent/src/client/page/strategy_index.rs`, add:

```rust
impl PageStrategyIndex {
    pub fn lookup_fns(
        &self,
        provider: &str,
        model: &str,
        build_q: &Query<&crate::client::page::strategy_components::BuildRequestFn>,
        parse_q: &Query<&crate::client::page::strategy_components::ParseSseFn>,
        env_q: &Query<&crate::client::page::strategy_components::EnvVarName>,
        endpoint_q: &Query<&crate::client::page::strategy_components::Endpoint>,
    ) -> Option<(
        crate::client::page::strategy_components::BuildRequest,
        crate::client::page::strategy_components::ParseSse,
        &'static str,
        String,
    )> {
        let e = self.get(provider, model)?;
        let build = build_q.get(e).ok()?.0;
        let parse = parse_q.get(e).ok()?.0;
        let env = env_q.get(e).ok()?.0;
        let endpoint = endpoint_q.get(e).ok()?.0.clone();
        Some((build, parse, env, endpoint))
    }
}
```

Add a test:

```rust
#[test]
fn lookup_fns_returns_none_for_unknown_key() {
    let idx = PageStrategyIndex::default();
    // construct empty queries — easiest via a tiny App
    // (skip if Query construction is awkward; rely on integration test)
}
```

If constructing `Query` outside an App is awkward, skip the unit test and rely on integration coverage in Task 3.4.

- [ ] **Step 3: Refactor `process_user_input`**

In `crates/vmux_agent/src/systems/process_input.rs`, replace the `strategies.get_page_by_provider_model(...)` line + downstream `strategy.build_request(...)` / `strategy.parse_sse_fn()` calls with:

```rust
let Some((build, parse, env_var, _endpoint)) = idx.lookup_fns(
    &session.provider,
    &session.model,
    &build_q,
    &parse_q,
    &env_q,
    &endpoint_q,
) else {
    bevy::log::warn!(
        "no Page strategy entity for {}/{}",
        session.provider,
        session.model
    );
    continue;
};
let api_key = std::env::var(env_var).unwrap_or_default();
let request = build(&session.model, &messages, &tools, &api_key);
// ... existing spawn that calls drive_sse(request, parse, tx) ...
```

Add the four queries + `idx: Res<PageStrategyIndex>` to the system signature. Drop `strategies: Res<AgentStrategies>` only if it has no other callers in this system.

- [ ] **Step 4: Same change in `continue_after_tool.rs`**

Mirror the refactor.

- [ ] **Step 5: Build + test**

```
cargo fmt -p vmux_agent
env -u CEF_PATH cargo clippy -p vmux_agent --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_agent
```
Expected: all green. Tests that mock strategies via `MockPageStrategy` may need rewriting — see Task 3.4.

- [ ] **Step 6: Commit (do not push yet — see 3.4)**

```
git add crates/vmux_agent/
git commit -m "refactor(vmux_agent): consumers look up Page strategies via ECS"
```

---

### Task 3.4: Migrate mock strategies in tests

**Files:**
- Modify: `crates/vmux_agent/src/systems/process_input.rs` (tests mod)
- Modify: `crates/vmux_agent/src/systems/continue_after_tool.rs` (tests mod)

- [ ] **Step 1: Replace `MockPageStrategy` impls**

Where tests build an App and call `app.world_mut().resource_mut::<AgentStrategies>().register_page(Arc::new(MockPageStrategy {..}))`, replace with spawning an entity:

```rust
fn mock_parse(_: &str) -> Option<StreamEvent> {
    None
}
fn mock_build(
    _: &str,
    _: &[crate::message::Message],
    _: &[crate::stream::ToolDef],
    _: &str,
) -> reqwest::Request {
    reqwest::Client::new()
        .get("http://localhost/")
        .build()
        .unwrap()
}

app.world_mut().spawn((
    Strategy,
    StrategyKey { provider: "mistral".into(), model: "devstral-2".into() },
    Endpoint("stub://".into()),
    EnvVarName(""),
    StrategyKind(AgentKind::Vibe),
    StrategyVariant(AgentVariant::Page),
    BuildRequestFn(mock_build),
    ParseSseFn(mock_parse),
));
// Run one update so the observer indexes it.
app.update();
```

- [ ] **Step 2: Build + test**

```
env -u CEF_PATH cargo test -p vmux_agent
```
Expected: all green.

- [ ] **Step 3: Squash into prior commit**

```
git add crates/vmux_agent/src/systems/
git commit --amend --no-edit
```

(Or keep as a separate commit if review prefers smaller diffs.)

- [ ] **Step 4: Run pre-push checks for changed crates**

```
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS { cargo fmt -p $pkg -- --check }
for pkg in $PKGS { with-env { CEF_PATH: null } { cargo clippy -p $pkg --all-targets -- -D warnings } }
for pkg in $PKGS { with-env { CEF_PATH: null } { cargo test -p $pkg } }
```
Expected: all green.

- [ ] **Step 5: Push (optional checkpoint PR)**

```
git push origin HEAD --force-with-lease
```

---

## Phase 4: Migrate Remaining Providers

Repeat Task 3.1 + 3.2 shape for each:

### Task 4.1: Anthropic

**Files:**
- Modify: `crates/vmux_agent/src/providers/anthropic.rs` — extract `pub fn build_request`, `pub fn parse_sse`, `pub const PROVIDER/ENDPOINT/ENV_VAR`.
- Create: `crates/vmux_agent/src/providers/anthropic_plugin.rs` — `AnthropicPlugin` mirroring `MistralPlugin`.
- Modify: `crates/vmux_agent/src/providers.rs` — `pub mod anthropic_plugin;`.
- Modify: `crates/vmux_agent/src/client/page/plugin.rs` — `app.add_plugins(crate::providers::anthropic_plugin::AnthropicPlugin)`.

- [ ] Apply changes following the Mistral pattern.
- [ ] `env -u CEF_PATH cargo test -p vmux_agent providers::anthropic`
- [ ] Commit: `feat(vmux_agent): AnthropicPlugin spawns Strategy entity`

### Task 4.2: OpenAI

Same pattern with `crates/vmux_agent/src/providers/openai.rs` + `openai_plugin.rs`. Default model `"gpt-5"`.

- [ ] Apply.
- [ ] Test.
- [ ] Commit: `feat(vmux_agent): OpenAiPlugin spawns Strategy entity`

### Task 4.3: Echo fallback

**Files:**
- Modify: `crates/vmux_agent/src/echo.rs` — add `pub fn build_request`, `pub fn parse_sse`, `pub const PROVIDER="echo"`, `pub const ENDPOINT="stub://echo"`, `pub const ENV_VAR=""`, `pub const DEFAULT_MODEL="echo"`.
- Create: `crates/vmux_agent/src/echo_plugin.rs` — `EchoPlugin` that ALWAYS spawns the entity (no env var check), running AFTER the other provider plugins so real providers win the `(provider, model)` slot if both somehow target the same key (they don't, but defensive).
- Modify: `crates/vmux_agent/src/lib.rs` — `pub mod echo_plugin;`
- Modify: `crates/vmux_agent/src/client/page/plugin.rs` — `app.add_plugins(crate::echo_plugin::EchoPlugin)`.

- [ ] Apply.
- [ ] Test that `EchoPlugin` spawns the entity even when no env vars set.
- [ ] Commit: `feat(vmux_agent): EchoPlugin spawns fallback Strategy entity`

---

## Phase 5: Delete the Trait

### Task 5.1: Remove `AgentPageStrategy` trait

**Files:**
- Delete: `crates/vmux_agent/src/client/page/strategy.rs`
- Modify: `crates/vmux_agent/src/client/page.rs` — remove `pub mod strategy;`
- Modify: `crates/vmux_agent/src/lib.rs` — remove `pub use client::page::strategy::AgentPageStrategy;`
- Modify: `crates/vmux_agent/src/strategy.rs` — delete `page` HashMap, `register_page*`, `get_page_by_provider_model`, `page_strategies`. Keep CLI side.
- Modify: provider files — drop `impl AgentPageStrategy for ...` and the `MistralStrategy` / `AnthropicStrategy` / `OpenAiResponsesStrategy` / `EchoPageStrategy` structs **only if they have no remaining callers**. Verify with `grep`.
- Modify: `crates/vmux_agent/src/providers.rs` — drop `pub use {mistral::MistralStrategy, ...}` lines that no longer compile.
- Modify: `crates/vmux_agent/src/providers/builtin.rs` — delete `instantiate_builtin` (no longer needed since plugins spawn entities directly). Keep `BUILTIN_PROVIDERS` only if other consumers exist; otherwise delete it too.
- Modify: `crates/vmux_agent/src/client/page/plugin.rs` — delete `register_builtin_providers` startup system (replaced by per-provider plugins).
- Modify: `crates/vmux_agent/src/plugin.rs` — `attach_page_agent_to_stack` and `spawn_page_agent_tab` currently call `strategies.get_page_by_provider_model(provider, model)?` just to validate registration. Replace with `idx.get(provider, model).is_some()` using `Res<PageStrategyIndex>`.

- [ ] **Step 1: Update `attach_page_agent_to_stack`**

```rust
pub fn attach_page_agent_to_stack(
    stack: Entity,
    provider: &str,
    model: &str,
    sid: &str,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    idx: &PageStrategyIndex,
    kind_q: &Query<&StrategyKind>,
) -> Option<()> {
    let e = idx.get(provider, model)?;
    let kind = kind_q.get(e).ok()?.0;
    // ... rest unchanged ...
}
```

Update the call signature in `spawn_page_agent_tab` and the two responder systems (`respond_page_agent_attach_default`, `respond_page_agent_spawn_default`, `respond_page_agent_spawn_tab`, `respond_page_agent_attach`) to pass `idx` + `kind_q` instead of `strategies`.

- [ ] **Step 2: Delete trait file + map**

Remove `client/page/strategy.rs` and the `page` field from `AgentStrategies`.

- [ ] **Step 3: Delete unused provider structs**

For each provider, if `grep -rn 'MistralStrategy\|AnthropicStrategy\|OpenAiResponsesStrategy\|EchoPageStrategy' crates/` returns no live callers, delete the struct + `impl AgentStrategy` + `impl AgentPageStrategy` blocks. Keep only the `pub fn build_request`, `pub fn parse_sse`, `pub const`s.

- [ ] **Step 4: Build + test**

```
cargo fmt -p vmux_agent
env -u CEF_PATH cargo clippy -p vmux_agent --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_agent
```
Expected: all green.

- [ ] **Step 5: Run pre-push checks across changed crates**

```
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS { cargo fmt -p $pkg -- --check }
for pkg in $PKGS { with-env { CEF_PATH: null } { cargo clippy -p $pkg --all-targets -- -D warnings } }
for pkg in $PKGS { with-env { CEF_PATH: null } { cargo test -p $pkg } }
```

- [ ] **Step 6: Commit**

```
git add -A
git commit -m "refactor(vmux_agent): remove AgentPageStrategy trait

Per-provider behavior now lives entirely in free functions registered
on Strategy entities. Lookup goes through PageStrategyIndex. The trait
and the page HashMap on AgentStrategies are gone."
```

---

## Phase 6 (Optional Follow-Up): Per-Strategy State Components

Once the trait is gone, ECS gains pay off. Each is independent.

### Task 6.1: Error count + last-used telemetry

- Add `ErrorCount(pub u32)` and `LastUsedAt(pub bevy::utils::Instant)` components to `strategy_components.rs`.
- Spawn with `ErrorCount(0)` and `LastUsedAt(Instant::now())` in each provider plugin.
- A system in `surface_errors` increments `ErrorCount` on `StreamEvent::Error` and updates `LastUsedAt` whenever a stream completes.
- Reflect both via `app.register_type::<ErrorCount>()` so they show in `bevy-inspector`.

### Task 6.2: Settings-driven providers become entities

- Replace `register_page_agents_from_settings` with `spawn_page_strategy_from_settings` that creates one Entity per `settings.agent.app_providers[].models[]` using `EchoPlugin`'s pattern (free fn pointers from `echo.rs` since user-defined providers currently fall back to echo).
- Removes the bespoke `EchoPageStrategy::new(provider, model, kind)` constructor path.

### Task 6.3: Hot-reload providers via settings watcher

- Settings watcher despawns + respawns Strategy entities when `settings.agent.app_providers` changes. The indexer observers keep `PageStrategyIndex` consistent automatically.

---

## Self-Review Notes

- All five providers (`mistral`, `anthropic`, `openai`, `echo`) gain matching `pub fn build_request`, `pub fn parse_sse`, and constants — see Phase 3.1 and 4.x.
- `drive_sse` signature change (Phase 2.1) coordinates with provider `parse_sse_fn()` (Phase 2.2) — both land in the same commit.
- Test mocks in `process_input`/`continue_after_tool` get rewritten to spawn entities (Phase 3.4); no test relies on `Arc<dyn AgentPageStrategy>` after Phase 5.
- `instantiate_builtin` and `BUILTIN_PROVIDERS` are deleted in Phase 5 only after every provider has its own plugin; intermediate phases keep them alive.
- `attach_page_agent_to_stack` / `spawn_page_agent_tab` are public APIs called from `vmux_layout` indirectly via messages. Their signature change (added query/resource params) stays inside `vmux_agent`; the message-driven boundary in `vmux_core` is unaffected.

---

## Execution Handoff

Two options:

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per phase (or per task within a phase), review between tasks.
2. **Inline Execution** — execute in the current session with checkpoints after each phase.

Pick one when ready to start.
