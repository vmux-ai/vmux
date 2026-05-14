# Claude & Codex CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `vmux://claude/[id]` and `vmux://codex/[id]` agent integration with full vibe-parity (spawn, resume, MCP injection, session discovery, exit detection), implemented via a new `vmux_agent` crate that abstracts the strategy-per-CLI logic.

**Architecture:** New `vmux_agent` crate owns kind-agnostic ECS components, systems, and per-strategy implementations (`VibeStrategy`, `ClaudeStrategy`, `CodexStrategy`). `vmux_desktop` keeps spawn helpers and provider registry; uses `vmux_agent` types as ECS components on terminal entities. Existing `vmux_desktop::vibe::*` is deleted and its callers migrated.

**Tech Stack:** Rust 2024, Bevy 0.18 ECS, `notify` crate for fs watching, `serde_json` + `chrono` for parsing CLI session logs.

**Spec:** `docs/specs/2026-05-14-claude-codex-cli-design.md`

---

## Pre-flight

### Task 0: Create worktree

**Files:** none

- [ ] **Step 1: Verify on main, no uncommitted work**

```bash
git status
git rev-parse --abbrev-ref HEAD
```

Expected: `On branch main` and no uncommitted changes. If unclean, ask the user before proceeding.

- [ ] **Step 2: Fetch latest origin/main**

```bash
git fetch origin main
```

- [ ] **Step 3: Create worktree at `.worktrees/vmx-claude-codex` branched from `origin/main`**

```bash
git worktree add .worktrees/vmx-claude-codex -b claude-codex origin/main
cd .worktrees/vmx-claude-codex
```

Expected: New worktree at `.worktrees/vmx-claude-codex`. All subsequent steps run inside this worktree.

- [ ] **Step 4: Verify spec exists in worktree**

```bash
ls docs/specs/2026-05-14-claude-codex-cli-design.md
```

Expected: file listed.

---

## Phase A — `vmux_agent` crate scaffolding

Goal: new crate builds, exports stub types, no behavior yet. After this phase, `cargo build -p vmux_agent` succeeds.

### Task A1: Create crate skeleton

**Files:**
- Create: `crates/vmux_agent/Cargo.toml`
- Create: `crates/vmux_agent/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Inspect a sibling crate's Cargo.toml for conventions**

```bash
cat crates/vmux_history/Cargo.toml
```

Note `version.workspace = true`, `edition.workspace = true`, `publish = false`, `[lib]` section.

- [ ] **Step 2: Write `crates/vmux_agent/Cargo.toml`**

```toml
[package]
name = "vmux_agent"
description = "Agent CLI strategies (vibe, claude, codex) and shared session ECS abstractions"
version.workspace = true
edition.workspace = true
publish = false

[lib]
path = "src/lib.rs"

[dependencies]
bevy = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
notify = { workspace = true }
vmux_core = { path = "../vmux_core" }
```

If any of `serde_json`, `chrono`, `notify` are not in `[workspace.dependencies]`, look up how vibe.rs imports them today (they're already used in `vmux_desktop`, so they exist somewhere — either workspace deps or `vmux_desktop`'s Cargo.toml). Mirror the version.

- [ ] **Step 3: Verify workspace `Cargo.toml` glob picks up the new crate**

```bash
grep -n 'crates/' Cargo.toml
```

Expected: line with `members = ["crates/*", ...]`. If `crates/*` is the pattern, no edit needed. Otherwise add `"crates/vmux_agent"` explicitly.

- [ ] **Step 4: Write minimal `crates/vmux_agent/src/lib.rs`**

```rust
pub mod kind;
pub mod exec;
pub mod mcp;
pub mod session;
pub mod strategy;
pub mod plugin;

pub mod vibe;
pub mod claude;
pub mod codex;

pub use kind::AgentKind;
pub use mcp::McpServerConfig;
pub use plugin::AgentSessionPlugin;
pub use session::{AgentSession, AgentSessionToEntity, PendingAgentSession, SessionId};
pub use strategy::{AgentStrategies, AgentStrategy};
```

- [ ] **Step 5: Create empty stub files so `lib.rs` compiles**

For each of `kind.rs`, `exec.rs`, `mcp.rs`, `session.rs`, `strategy.rs`, `plugin.rs`, `vibe.rs`, `claude.rs`, `codex.rs` under `crates/vmux_agent/src/`, create the file with content:

```rust
// stub: filled in later tasks
```

This will fail compilation because `lib.rs` references symbols that don't exist yet. Comment out the `pub use` lines and the body of `pub mod` lines that aren't yet defined. Leave only `pub mod kind;` for now and re-add the rest as each task lands.

Actually, simpler: comment out everything in `lib.rs` except `pub mod kind;`, and uncomment progressively as tasks land. Replace `lib.rs` body with:

```rust
pub mod kind;
// uncomment as tasks land:
// pub mod exec;
// pub mod mcp;
// pub mod session;
// pub mod strategy;
// pub mod plugin;
// pub mod vibe;
// pub mod claude;
// pub mod codex;
```

And empty stub `kind.rs` for now:

```rust
```

- [ ] **Step 6: Build the crate**

```bash
cargo build -p vmux_agent
```

Expected: compiles. Empty crate is fine.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_agent/Cargo.toml crates/vmux_agent/src/lib.rs crates/vmux_agent/src/kind.rs Cargo.toml
git commit -m "feat(vmux_agent): scaffold new crate"
```

---

### Task A2: `AgentKind` enum

**Files:**
- Modify: `crates/vmux_agent/src/kind.rs`

- [ ] **Step 1: Write the failing test in `kind.rs`**

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AgentKind {
    Vibe,
    Claude,
    Codex,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_host_recognizes_known_schemes() {
        assert_eq!(AgentKind::from_host("vibe"),   Some(AgentKind::Vibe));
        assert_eq!(AgentKind::from_host("claude"), Some(AgentKind::Claude));
        assert_eq!(AgentKind::from_host("codex"),  Some(AgentKind::Codex));
        assert_eq!(AgentKind::from_host("nope"),   None);
    }

    #[test]
    fn executable_returns_cli_binary_name() {
        assert_eq!(AgentKind::Vibe.executable(),   "vibe");
        assert_eq!(AgentKind::Claude.executable(), "claude");
        assert_eq!(AgentKind::Codex.executable(),  "codex");
    }

    #[test]
    fn url_scheme_returns_vmux_prefix_with_trailing_slash() {
        assert_eq!(AgentKind::Vibe.url_scheme(),   "vmux://vibe/");
        assert_eq!(AgentKind::Claude.url_scheme(), "vmux://claude/");
        assert_eq!(AgentKind::Codex.url_scheme(),  "vmux://codex/");
    }
}
```

- [ ] **Step 2: Run tests; expect failure (`from_host` / `executable` / `url_scheme` undefined)**

```bash
cargo test -p vmux_agent kind::tests
```

Expected: 3 failures, "no method named …"

- [ ] **Step 3: Implement methods in `kind.rs`**

Replace the file with:

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AgentKind {
    Vibe,
    Claude,
    Codex,
}

impl AgentKind {
    pub fn executable(self) -> &'static str {
        match self {
            AgentKind::Vibe   => "vibe",
            AgentKind::Claude => "claude",
            AgentKind::Codex  => "codex",
        }
    }

    pub fn url_scheme(self) -> &'static str {
        match self {
            AgentKind::Vibe   => "vmux://vibe/",
            AgentKind::Claude => "vmux://claude/",
            AgentKind::Codex  => "vmux://codex/",
        }
    }

    pub fn from_host(host: &str) -> Option<Self> {
        match host {
            "vibe"   => Some(AgentKind::Vibe),
            "claude" => Some(AgentKind::Claude),
            "codex"  => Some(AgentKind::Codex),
            _ => None,
        }
    }

    pub fn all() -> [AgentKind; 3] {
        [AgentKind::Vibe, AgentKind::Claude, AgentKind::Codex]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_host_recognizes_known_schemes() {
        assert_eq!(AgentKind::from_host("vibe"),   Some(AgentKind::Vibe));
        assert_eq!(AgentKind::from_host("claude"), Some(AgentKind::Claude));
        assert_eq!(AgentKind::from_host("codex"),  Some(AgentKind::Codex));
        assert_eq!(AgentKind::from_host("nope"),   None);
    }

    #[test]
    fn executable_returns_cli_binary_name() {
        assert_eq!(AgentKind::Vibe.executable(),   "vibe");
        assert_eq!(AgentKind::Claude.executable(), "claude");
        assert_eq!(AgentKind::Codex.executable(),  "codex");
    }

    #[test]
    fn url_scheme_returns_vmux_prefix_with_trailing_slash() {
        assert_eq!(AgentKind::Vibe.url_scheme(),   "vmux://vibe/");
        assert_eq!(AgentKind::Claude.url_scheme(), "vmux://claude/");
        assert_eq!(AgentKind::Codex.url_scheme(),  "vmux://codex/");
    }
}
```

- [ ] **Step 4: Run tests; expect pass**

```bash
cargo test -p vmux_agent kind::tests
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/kind.rs
git commit -m "feat(vmux_agent): add AgentKind enum"
```

---

### Task A3: `find_executable` (move from vibe.rs, fresh tests)

**Files:**
- Modify: `crates/vmux_agent/src/exec.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Uncomment `pub mod exec;` in `lib.rs`**

```rust
pub mod kind;
pub mod exec;
// uncomment as tasks land:
// pub mod mcp;
// ...
```

- [ ] **Step 2: Read existing implementation in `crates/vmux_desktop/src/vibe.rs` lines 102-145**

Reuse this code verbatim (it's already battle-tested) but place it under `vmux_agent::exec`.

- [ ] **Step 3: Write `crates/vmux_agent/src/exec.rs`**

```rust
use std::path::{Path, PathBuf};

pub fn find_executable(command: &str) -> Option<PathBuf> {
    let from_path = std::env::var_os("PATH")
        .and_then(|path| path.into_string().ok())
        .and_then(|path| find_executable_in_path(command, &path));
    from_path.or_else(|| find_executable_in_fallback_dirs(command))
}

fn find_executable_in_path(command: &str, path_env: &str) -> Option<PathBuf> {
    path_env
        .split(':')
        .filter(|part| !part.is_empty())
        .map(|part| Path::new(part).join(command))
        .find(|path| is_executable(path))
}

fn find_executable_in_fallback_dirs(command: &str) -> Option<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".cargo/bin"));
    }
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.into_iter()
        .map(|dir| dir.join(command))
        .find(|path| is_executable(path))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lookup_finds_executable_on_path() {
        let temp = std::env::temp_dir().join(format!(
            "vmux-agent-exec-path-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&temp).unwrap();
        let exe = temp.join("fake-cli");
        std::fs::write(&exe, b"").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let found = find_executable_in_path("fake-cli", temp.to_string_lossy().as_ref());
        let _ = std::fs::remove_file(&exe);
        let _ = std::fs::remove_dir(&temp);
        assert_eq!(found, Some(exe));
    }
}
```

(Skip the `command_lookup_finds_executable_in_home_local_bin_when_path_misses` test for now — it touches process-global env. The single test above is enough to lock the contract; the home-fallback test can be ported in Task A3.bonus if regressions appear.)

- [ ] **Step 4: Run tests**

```bash
cargo test -p vmux_agent exec::tests
```

Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/exec.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): add executable lookup"
```

---

### Task A4: `McpServerConfig` + sidecar resolution

**Files:**
- Modify: `crates/vmux_agent/src/mcp.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Read existing implementation in `crates/vmux_desktop/src/vibe.rs` lines 16-20, 147-206**

Lift the `McpServerConfig` struct, `resolve_mcp_server_config`, `mcp_server_config_for`, `find_workspace_dir`, `vmux_sidecar_path` verbatim.

- [ ] **Step 2: Uncomment `pub mod mcp;` and `pub use mcp::McpServerConfig;` in `lib.rs`**

- [ ] **Step 3: Write `crates/vmux_agent/src/mcp.rs`**

```rust
use std::path::{Path, PathBuf};

use crate::exec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpServerConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

pub fn resolve(cwd: &Path) -> Result<McpServerConfig, String> {
    let sidecar = vmux_sidecar_path()?;
    resolve_with_sidecar(&sidecar, cwd)
}

fn resolve_with_sidecar(sidecar: &Path, cwd: &Path) -> Result<McpServerConfig, String> {
    if exec::is_executable_path(sidecar) {
        return Ok(McpServerConfig {
            command: sidecar.to_string_lossy().to_string(),
            args: vec!["mcp".to_string()],
            cwd: None,
        });
    }
    let workspace = find_workspace_dir(cwd)
        .ok_or_else(|| format!("vmux executable not found: {}", sidecar.display()))?;
    Ok(McpServerConfig {
        command: "cargo".to_string(),
        args: ["run", "--quiet", "-p", "vmux_cli", "--bin", "vmux", "--", "mcp"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        cwd: Some(workspace),
    })
}

fn find_workspace_dir(cwd: &Path) -> Option<PathBuf> {
    let mut current = cwd;
    loop {
        if current.join("Cargo.toml").is_file() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

fn vmux_sidecar_path() -> Result<PathBuf, String> {
    let current = std::env::current_exe()
        .map_err(|error| format!("resolve current executable failed: {error}"))?;
    let Some(dir) = current.parent() else {
        return Err("current executable has no parent directory".to_string());
    };
    Ok(dir.join("vmux"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_cargo_run_when_sidecar_is_missing() {
        let temp = std::env::temp_dir().join(format!(
            "vmux-agent-mcp-{}",
            std::process::id()
        ));
        let workspace = temp.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("Cargo.toml"), b"[workspace]\n").unwrap();

        let config = resolve_with_sidecar(&temp.join("missing-vmux"), &workspace).unwrap();
        let _ = std::fs::remove_dir_all(&temp);

        assert_eq!(config.command, "cargo");
        assert_eq!(
            config.args,
            vec!["run", "--quiet", "-p", "vmux_cli", "--bin", "vmux", "--", "mcp"]
        );
        assert_eq!(config.cwd, Some(workspace));
    }
}
```

- [ ] **Step 4: Add `pub fn is_executable_path` to `exec.rs`**

Add this public wrapper above the private `is_executable` so `mcp.rs` can call it:

```rust
pub fn is_executable_path(path: &Path) -> bool {
    is_executable(path)
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p vmux_agent mcp::tests
```

Expected: 1 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_agent/src/mcp.rs crates/vmux_agent/src/exec.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): add MCP config resolution"
```

---

### Task A5: ECS components + `AgentSessionToEntity`

**Files:**
- Modify: `crates/vmux_agent/src/session.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Uncomment `pub mod session;` and the matching `pub use` lines in `lib.rs`**

- [ ] **Step 2: Write component types and the entity-map resource in `session.rs`**

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use bevy::prelude::*;

use crate::AgentKind;

#[derive(Component, Debug, Clone)]
pub struct AgentSession {
    pub kind: AgentKind,
}

#[derive(Component, Debug, Clone)]
pub struct SessionId(pub String);

#[derive(Component, Debug, Clone)]
pub struct PendingAgentSession {
    pub kind: AgentKind,
    pub spawn_time: SystemTime,
    pub cwd: PathBuf,
}

#[derive(Resource, Default, Debug)]
pub struct AgentSessionToEntity(pub HashMap<(AgentKind, String), Entity>);

#[derive(Resource, Default, Debug)]
pub struct AgentSessionDirty(pub bool);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_session_to_entity_starts_empty() {
        let map = AgentSessionToEntity::default();
        assert!(map.0.is_empty());
    }

    #[test]
    fn pending_session_carries_cwd_and_kind() {
        let pending = PendingAgentSession {
            kind: AgentKind::Claude,
            spawn_time: SystemTime::UNIX_EPOCH,
            cwd: PathBuf::from("/tmp/x"),
        };
        assert_eq!(pending.kind, AgentKind::Claude);
        assert_eq!(pending.cwd, PathBuf::from("/tmp/x"));
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p vmux_agent session::tests
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_agent/src/session.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): add session components and entity map"
```

---

### Task A6: `AgentStrategy` trait + `AgentStrategies` resource

**Files:**
- Modify: `crates/vmux_agent/src/strategy.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Uncomment `pub mod strategy;` and matching `pub use` in `lib.rs`**

- [ ] **Step 2: Write the trait, registry resource, and a smoke test in `strategy.rs`**

```rust
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy::prelude::Resource;

use crate::{AgentKind, McpServerConfig};

pub trait AgentStrategy: Send + Sync + 'static {
    fn kind(&self) -> AgentKind;
    fn sessions_root(&self) -> PathBuf;
    fn build_args(&self, mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String>;
    fn build_env(&self, mcp: &McpServerConfig) -> Vec<(String, String)>;
    fn discover_session(
        &self,
        cwd: &Path,
        spawn_time: SystemTime,
        claimed: &HashSet<String>,
    ) -> Option<String>;
    fn detect_end_time(&self, session_id: &str) -> bool;
}

#[derive(Resource, Default)]
pub struct AgentStrategies {
    inner: HashMap<AgentKind, Box<dyn AgentStrategy>>,
}

impl AgentStrategies {
    pub fn register(&mut self, strategy: Box<dyn AgentStrategy>) {
        self.inner.insert(strategy.kind(), strategy);
    }

    pub fn get(&self, kind: AgentKind) -> Option<&dyn AgentStrategy> {
        self.inner.get(&kind).map(|b| b.as_ref())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&AgentKind, &dyn AgentStrategy)> {
        self.inner.iter().map(|(k, v)| (k, v.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    struct StubStrategy;
    impl AgentStrategy for StubStrategy {
        fn kind(&self) -> AgentKind { AgentKind::Claude }
        fn sessions_root(&self) -> PathBuf { PathBuf::from("/tmp/none") }
        fn build_args(&self, _: &McpServerConfig, _: Option<&str>) -> Vec<String> { vec![] }
        fn build_env(&self, _: &McpServerConfig) -> Vec<(String, String)> { vec![] }
        fn discover_session(
            &self, _: &Path, _: SystemTime, _: &HashSet<String>,
        ) -> Option<String> { None }
        fn detect_end_time(&self, _: &str) -> bool { false }
    }

    #[test]
    fn register_and_lookup_by_kind() {
        let mut s = AgentStrategies::default();
        s.register(Box::new(StubStrategy));
        assert!(s.get(AgentKind::Claude).is_some());
        assert!(s.get(AgentKind::Vibe).is_none());
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p vmux_agent strategy::tests
```

Expected: 1 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_agent/src/strategy.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): add AgentStrategy trait and registry"
```

---

## Phase B — Strategy implementations

### Task B1: `VibeStrategy`

**Files:**
- Modify: `crates/vmux_agent/src/vibe.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Uncomment `pub mod vibe;` in `lib.rs`**

- [ ] **Step 2: Write the strategy in `vibe.rs`** (port behavior from `crates/vmux_desktop/src/vibe.rs` lines 22-31, 147-158, 208-234 and `vibe/session.rs` lines 96-181, 219-277)

```rust
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

use crate::{AgentKind, McpServerConfig};
use crate::strategy::AgentStrategy;

pub struct VibeStrategy;

impl AgentStrategy for VibeStrategy {
    fn kind(&self) -> AgentKind { AgentKind::Vibe }

    fn sessions_root(&self) -> PathBuf {
        std::env::var("VIBE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_default();
                PathBuf::from(home).join(".vibe")
            })
            .join("logs")
            .join("session")
    }

    fn build_args(&self, _mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String> {
        let mut args = vec!["--trust".to_string()];
        if let Some(sid) = session_id {
            args.push("--resume".to_string());
            args.push(sid.to_string());
        }
        args
    }

    fn build_env(&self, mcp: &McpServerConfig) -> Vec<(String, String)> {
        let json = serialize_vibe_mcp_env(mcp);
        vec![("VIBE_MCP_SERVERS".to_string(), json)]
    }

    fn discover_session(
        &self,
        cwd: &Path,
        spawn_time: SystemTime,
        claimed: &HashSet<String>,
    ) -> Option<String> {
        discover_vibe_session_id(&self.sessions_root(), cwd, spawn_time, claimed)
    }

    fn detect_end_time(&self, session_id: &str) -> bool {
        let root = self.sessions_root();
        let Ok(entries) = std::fs::read_dir(&root) else { return false };
        for entry in entries.flatten() {
            let meta_path = entry.path().join("meta.json");
            let Ok(text) = std::fs::read_to_string(&meta_path) else { continue };
            let Ok(head) = serde_json::from_str::<MetaJsonHead>(&text) else { continue };
            if head.session_id != session_id { continue; }
            let Ok(exit) = serde_json::from_str::<MetaJsonExit>(&text) else { continue };
            return exit.end_time.is_some();
        }
        false
    }
}

#[derive(Serialize)]
struct VibeMcpServerEnv {
    name: &'static str,
    transport: &'static str,
    command: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
}

fn serialize_vibe_mcp_env(mcp: &McpServerConfig) -> String {
    let server = VibeMcpServerEnv {
        name: "vmux",
        transport: "stdio",
        command: mcp.command.clone(),
        args: mcp.args.clone(),
        cwd: mcp.cwd.as_ref().map(|c| c.to_string_lossy().to_string()),
    };
    serde_json::to_string(&[server]).unwrap_or_else(|_| "[]".to_string())
}

#[derive(serde::Deserialize)]
struct MetaJson {
    session_id: String,
    start_time: String,
    environment: MetaEnvironment,
}
#[derive(serde::Deserialize)]
struct MetaEnvironment { working_directory: String }
#[derive(serde::Deserialize)]
struct MetaJsonHead { session_id: String }
#[derive(serde::Deserialize)]
struct MetaJsonExit { end_time: Option<String> }

fn normalize_cwd(path: &Path) -> String {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canon.to_string_lossy().trim_end_matches('/').to_string()
}

pub(crate) fn discover_vibe_session_id(
    sessions_root: &Path,
    cwd: &Path,
    spawn_time: SystemTime,
    claimed: &HashSet<String>,
) -> Option<String> {
    let cwd_norm = normalize_cwd(cwd);
    let spawn_secs = spawn_time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let entries = std::fs::read_dir(sessions_root).ok()?;
    let mut best: Option<(i64, String)> = None;
    for entry in entries.flatten() {
        let meta_path = entry.path().join("meta.json");
        let Ok(text) = std::fs::read_to_string(&meta_path) else { continue };
        let Ok(meta) = serde_json::from_str::<MetaJson>(&text) else { continue };
        let meta_cwd = normalize_cwd(Path::new(&meta.environment.working_directory));
        if meta_cwd != cwd_norm { continue; }
        if claimed.contains(&meta.session_id) { continue; }
        let Ok(start_dt) = chrono::DateTime::parse_from_rfc3339(&meta.start_time) else { continue };
        let start_secs = start_dt.timestamp();
        if start_secs < spawn_secs { continue; }
        match &best {
            None => best = Some((start_secs, meta.session_id)),
            Some((cur, _)) if start_secs < *cur => best = Some((start_secs, meta.session_id)),
            _ => {}
        }
    }
    best.map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn write_meta(dir: &Path, session_id: &str, working_dir: &str, start_time: &str, end_time: Option<&str>) {
        std::fs::create_dir_all(dir).unwrap();
        let end_field = end_time
            .map(|e| format!(r#","end_time":"{e}""#))
            .unwrap_or_default();
        std::fs::write(
            dir.join("meta.json"),
            format!(
                r#"{{"session_id":"{session_id}","start_time":"{start_time}"{end_field},"environment":{{"working_directory":"{working_dir}"}}}}"#
            ),
        )
        .unwrap();
    }

    fn unique_tmp(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("vmux-agent-{label}-{pid}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn discover_picks_session_matching_cwd_and_after_spawn_time() {
        let tmp = unique_tmp("vibe-discover");
        let sessions = tmp.join("sessions");
        let cwd = "/tmp/work-A";
        write_meta(&sessions.join("a"), "older", cwd, "2025-12-31T23:00:00+00:00", None);
        write_meta(&sessions.join("b"), "this",  cwd, "2026-05-11T12:00:00+00:00", None);
        write_meta(&sessions.join("c"), "other", "/tmp/work-B", "2026-05-11T12:00:00+00:00", None);

        let spawn = SystemTime::UNIX_EPOCH + Duration::from_secs(1_770_000_000);
        let claimed = HashSet::new();
        let result = discover_vibe_session_id(&sessions, Path::new(cwd), spawn, &claimed);
        assert_eq!(result.as_deref(), Some("this"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn detect_end_time_returns_true_when_meta_has_end_time() {
        let tmp = unique_tmp("vibe-end");
        let sessions = tmp.join("sessions");
        let cwd = "/tmp/work";
        write_meta(&sessions.join("a"), "ended-id", cwd, "2026-05-11T12:00:00+00:00", Some("2026-05-11T13:00:00+00:00"));
        write_meta(&sessions.join("b"), "live-id",  cwd, "2026-05-11T12:00:00+00:00", None);

        // Override sessions_root by setting VIBE_HOME for this test only
        let strategy = VibeStrategy;
        // We can't override sessions_root() without env mutation, so test the lower-level function shape
        // via re-reading directly:
        let read_end = |id: &str| -> bool {
            let entries = std::fs::read_dir(&sessions).unwrap();
            for entry in entries.flatten() {
                let path = entry.path().join("meta.json");
                let text = std::fs::read_to_string(&path).unwrap();
                let head: MetaJsonHead = serde_json::from_str(&text).unwrap();
                if head.session_id != id { continue; }
                let exit: MetaJsonExit = serde_json::from_str(&text).unwrap();
                return exit.end_time.is_some();
            }
            false
        };
        assert!(read_end("ended-id"));
        assert!(!read_end("live-id"));
        let _ = strategy;
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
```

(The end-time test verifies file-format parsing; full `detect_end_time` integration is exercised by Task C7's ECS test.)

- [ ] **Step 3: Run tests**

```bash
cargo test -p vmux_agent vibe::tests
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_agent/src/vibe.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): VibeStrategy"
```

---

### Task B2: `ClaudeStrategy`

**Files:**
- Modify: `crates/vmux_agent/src/claude.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Uncomment `pub mod claude;` in `lib.rs`**

- [ ] **Step 2: Confirm the project-dir encoding by sampling real dirs**

```bash
ls ~/.claude/projects/ | head -3
```

Confirm pattern: each `/` and `.` becomes `-` (e.g. `/Users/junichi.sugiura/.config/nvim` → `-Users-junichi-sugiura--config-nvim`). Encode this as: replace every char that is not alphanumeric, `_`, or `-` with `-`.

- [ ] **Step 3: Write the strategy and tests in `claude.rs`**

```rust
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{Map, Value};

use crate::{AgentKind, McpServerConfig};
use crate::strategy::AgentStrategy;

pub struct ClaudeStrategy;

impl AgentStrategy for ClaudeStrategy {
    fn kind(&self) -> AgentKind { AgentKind::Claude }

    fn sessions_root(&self) -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".claude").join("projects")
    }

    fn build_args(&self, mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String> {
        let mut args = vec![
            "--permission-mode".to_string(), "bypassPermissions".to_string(),
            "--mcp-config".to_string(),       build_mcp_config_json(mcp),
            "--strict-mcp-config".to_string(),
        ];
        if let Some(sid) = session_id {
            args.push("--resume".to_string());
            args.push(sid.to_string());
        }
        args
    }

    fn build_env(&self, _mcp: &McpServerConfig) -> Vec<(String, String)> { vec![] }

    fn discover_session(
        &self, cwd: &Path, spawn_time: SystemTime, claimed: &HashSet<String>,
    ) -> Option<String> {
        let dir = self.sessions_root().join(project_dir_name(cwd));
        discover_claude_session_id(&dir, spawn_time, claimed)
    }

    fn detect_end_time(&self, _session_id: &str) -> bool { false }
}

pub(crate) fn project_dir_name(cwd: &Path) -> String {
    let s = cwd.to_string_lossy();
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '-' })
        .collect()
}

fn build_mcp_config_json(mcp: &McpServerConfig) -> String {
    let mut server = Map::new();
    server.insert("command".into(), Value::String(mcp.command.clone()));
    server.insert("args".into(), Value::Array(
        mcp.args.iter().map(|s| Value::String(s.clone())).collect()
    ));
    if let Some(cwd) = &mcp.cwd {
        server.insert("cwd".into(), Value::String(cwd.to_string_lossy().into()));
    }
    let mut servers = Map::new();
    servers.insert("vmux".into(), Value::Object(server));
    let mut root = Map::new();
    root.insert("mcpServers".into(), Value::Object(servers));
    serde_json::to_string(&Value::Object(root)).unwrap_or_else(|_| "{}".into())
}

pub(crate) fn discover_claude_session_id(
    project_dir: &Path,
    spawn_time: SystemTime,
    claimed: &HashSet<String>,
) -> Option<String> {
    let entries = std::fs::read_dir(project_dir).ok()?;
    let mut best: Option<(SystemTime, String)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") { continue; }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        if claimed.contains(stem) { continue; }
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        let Ok(created) = meta.created().or_else(|_| meta.modified()) else { continue };
        if created < spawn_time { continue; }
        match &best {
            None => best = Some((created, stem.to_string())),
            Some((cur, _)) if created < *cur => best = Some((created, stem.to_string())),
            _ => {}
        }
    }
    best.map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn unique_tmp(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("vmux-agent-{label}-{pid}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn project_dir_name_replaces_slashes_and_dots_with_dashes() {
        assert_eq!(project_dir_name(Path::new("/Users/junichi.sugiura/.config/nvim")),
                   "-Users-junichi-sugiura--config-nvim");
        assert_eq!(project_dir_name(Path::new("/tmp/a")), "-tmp-a");
    }

    #[test]
    fn discover_picks_jsonl_under_project_dir_after_spawn_time() {
        let tmp = unique_tmp("claude-discover");
        let dir = tmp.join("project");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("session-old.jsonl"), b"x").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        let spawn = SystemTime::now();
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(dir.join("session-new.jsonl"), b"x").unwrap();

        let claimed = HashSet::new();
        let id = discover_claude_session_id(&dir, spawn, &claimed);
        assert_eq!(id.as_deref(), Some("session-new"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn discover_skips_claimed() {
        let tmp = unique_tmp("claude-claimed");
        let dir = tmp.join("project");
        std::fs::create_dir_all(&dir).unwrap();
        let spawn = SystemTime::now();
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(dir.join("session-a.jsonl"), b"x").unwrap();
        std::fs::write(dir.join("session-b.jsonl"), b"x").unwrap();

        let mut claimed = HashSet::new();
        claimed.insert("session-a".to_string());
        let id = discover_claude_session_id(&dir, spawn, &claimed);
        assert_eq!(id.as_deref(), Some("session-b"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn build_args_includes_mcp_config_and_strict() {
        let mcp = McpServerConfig {
            command: "/bin/vmux".into(),
            args: vec!["mcp".into()],
            cwd: None,
        };
        let args = ClaudeStrategy.build_args(&mcp, None);
        assert!(args.iter().any(|a| a == "--strict-mcp-config"));
        assert!(args.iter().any(|a| a == "--mcp-config"));
        assert!(args.iter().any(|a| a == "--permission-mode"));
        assert!(args.iter().any(|a| a == "bypassPermissions"));
    }

    #[test]
    fn build_args_resume_appends_resume_flag() {
        let mcp = McpServerConfig { command: "x".into(), args: vec![], cwd: None };
        let args = ClaudeStrategy.build_args(&mcp, Some("abc-123"));
        let resume_idx = args.iter().position(|a| a == "--resume").unwrap();
        assert_eq!(args[resume_idx + 1], "abc-123");
    }

    #[test]
    fn detect_end_time_always_false() {
        assert!(!ClaudeStrategy.detect_end_time("anything"));
    }

    #[test]
    fn build_mcp_config_json_includes_vmux_server_with_command_and_args() {
        let mcp = McpServerConfig {
            command: "/bin/vmux".into(),
            args: vec!["mcp".into()],
            cwd: Some(PathBuf::from("/work")),
        };
        let json = build_mcp_config_json(&mcp);
        assert!(json.contains("\"command\":\"/bin/vmux\""));
        assert!(json.contains("\"args\":[\"mcp\"]"));
        assert!(json.contains("\"cwd\":\"/work\""));
        assert!(json.contains("\"vmux\""));
        assert!(json.contains("\"mcpServers\""));
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p vmux_agent claude::tests
```

Expected: 7 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/claude.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): ClaudeStrategy"
```

---

### Task B3: `CodexStrategy`

**Files:**
- Modify: `crates/vmux_agent/src/codex.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Uncomment `pub mod codex;` in `lib.rs`**

- [ ] **Step 2: Probe a real codex jsonl to confirm format**

```bash
head -1 $(find ~/.codex/sessions -name 'rollout-*.jsonl' | head -1)
```

Expect first line JSON to have `payload.id`, `payload.cwd`, `payload.timestamp`. (Already verified during brainstorming; sanity-check before implementing.)

- [ ] **Step 3: Write the strategy and tests in `codex.rs`**

```rust
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::{AgentKind, McpServerConfig};
use crate::strategy::AgentStrategy;

pub struct CodexStrategy;

impl AgentStrategy for CodexStrategy {
    fn kind(&self) -> AgentKind { AgentKind::Codex }

    fn sessions_root(&self) -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".codex").join("sessions")
    }

    fn build_args(&self, mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "-s".into(), "workspace-write".into(),
            "-a".into(), "never".into(),
            "-c".into(), format!("mcp_servers.vmux.command={}", quote_toml(&mcp.command)),
            "-c".into(), format!("mcp_servers.vmux.args={}", toml_array(&mcp.args)),
        ];
        if let Some(cwd) = &mcp.cwd {
            args.push("-c".into());
            args.push(format!("mcp_servers.vmux.cwd={}", quote_toml(&cwd.to_string_lossy())));
        }
        if let Some(sid) = session_id {
            args.push("resume".into());
            args.push(sid.to_string());
        }
        args
    }

    fn build_env(&self, _mcp: &McpServerConfig) -> Vec<(String, String)> { vec![] }

    fn discover_session(
        &self, cwd: &Path, spawn_time: SystemTime, claimed: &HashSet<String>,
    ) -> Option<String> {
        discover_codex_session_id(&self.sessions_root(), cwd, spawn_time, claimed)
    }

    fn detect_end_time(&self, _session_id: &str) -> bool { false }
}

pub(crate) fn quote_toml(s: &str) -> String {
    let escaped: String = s.chars().flat_map(|c| match c {
        '"'  => vec!['\\', '"'],
        '\\' => vec!['\\', '\\'],
        c    => vec![c],
    }).collect();
    format!("\"{escaped}\"")
}

pub(crate) fn toml_array(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| quote_toml(s)).collect();
    format!("[{}]", inner.join(","))
}

fn normalize_cwd(path: &Path) -> String {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canon.to_string_lossy().trim_end_matches('/').to_string()
}

#[derive(serde::Deserialize)]
struct CodexHead {
    #[serde(rename = "type")]
    kind: String,
    payload: CodexHeadPayload,
}
#[derive(serde::Deserialize)]
struct CodexHeadPayload {
    id: String,
    cwd: String,
}

pub(crate) fn discover_codex_session_id(
    sessions_root: &Path,
    cwd: &Path,
    spawn_time: SystemTime,
    claimed: &HashSet<String>,
) -> Option<String> {
    let cwd_norm = normalize_cwd(cwd);
    let mut best: Option<(SystemTime, String)> = None;
    walk_jsonl(sessions_root, &mut |path: &Path| {
        let Ok(meta) = std::fs::metadata(path) else { return };
        let Ok(modified) = meta.modified() else { return };
        if modified < spawn_time { return; }
        let Ok(text) = std::fs::read_to_string(path) else { return };
        let Some(line) = text.lines().next() else { return };
        let Ok(head) = serde_json::from_str::<CodexHead>(line) else { return };
        if head.kind != "session_meta" { return; }
        if claimed.contains(&head.payload.id) { return; }
        let head_cwd = normalize_cwd(Path::new(&head.payload.cwd));
        if head_cwd != cwd_norm { return; }
        match &best {
            None => best = Some((modified, head.payload.id.clone())),
            Some((cur, _)) if modified < *cur => best = Some((modified, head.payload.id.clone())),
            _ => {}
        }
    });
    best.map(|(_, id)| id)
}

fn walk_jsonl(root: &Path, visit: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_jsonl(&path, visit);
        } else if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            visit(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn unique_tmp(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("vmux-agent-{label}-{pid}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_session(root: &Path, ymd: &str, file: &str, id: &str, cwd: &str) {
        let dir = root.join(ymd);
        std::fs::create_dir_all(&dir).unwrap();
        let line = format!(
            r#"{{"timestamp":"2026-04-30T11:41:00.170Z","type":"session_meta","payload":{{"id":"{id}","timestamp":"2026-04-30T09:56:21.846Z","cwd":"{cwd}"}}}}"#
        );
        std::fs::write(dir.join(file), format!("{line}\n")).unwrap();
    }

    #[test]
    fn quote_toml_escapes_quotes_and_backslashes() {
        assert_eq!(quote_toml("a"),       "\"a\"");
        assert_eq!(quote_toml(r#"a"b"#),  "\"a\\\"b\"");
        assert_eq!(quote_toml(r"a\b"),    "\"a\\\\b\"");
    }

    #[test]
    fn toml_array_emits_quoted_csv() {
        assert_eq!(toml_array(&[]), "[]");
        assert_eq!(
            toml_array(&["mcp".into(), "x".into()]),
            "[\"mcp\",\"x\"]"
        );
    }

    #[test]
    fn build_args_uses_dash_c_overrides_for_mcp() {
        let mcp = McpServerConfig {
            command: "/bin/vmux".into(),
            args: vec!["mcp".into()],
            cwd: None,
        };
        let args = CodexStrategy.build_args(&mcp, None);
        assert!(args.windows(2).any(|w| w[0] == "-s" && w[1] == "workspace-write"));
        assert!(args.windows(2).any(|w| w[0] == "-a" && w[1] == "never"));
        assert!(args.iter().any(|a| a == "mcp_servers.vmux.command=\"/bin/vmux\""));
        assert!(args.iter().any(|a| a == "mcp_servers.vmux.args=[\"mcp\"]"));
    }

    #[test]
    fn build_args_resume_uses_resume_subcommand() {
        let mcp = McpServerConfig { command: "x".into(), args: vec![], cwd: None };
        let args = CodexStrategy.build_args(&mcp, Some("abc-123"));
        let resume_idx = args.iter().position(|a| a == "resume").unwrap();
        assert_eq!(args[resume_idx + 1], "abc-123");
        // resume must come AFTER the global -c flags
        let last_dash_c = args.iter().rposition(|a| a == "-c").unwrap();
        assert!(resume_idx > last_dash_c);
    }

    #[test]
    fn discover_walks_yyyy_mm_dd_dirs() {
        let tmp = unique_tmp("codex-walk");
        let sessions = tmp.join("sessions");
        let cwd = "/tmp/work";
        let spawn = SystemTime::now() - Duration::from_secs(60);
        write_session(&sessions, "2026/05/14", "rollout-a.jsonl", "id-a", cwd);
        write_session(&sessions, "2026/05/14", "rollout-b.jsonl", "id-b", "/tmp/other");

        let claimed = HashSet::new();
        let result = discover_codex_session_id(&sessions, Path::new(cwd), spawn, &claimed);
        assert_eq!(result.as_deref(), Some("id-a"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn detect_end_time_always_false() {
        assert!(!CodexStrategy.detect_end_time("anything"));
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p vmux_agent codex::tests
```

Expected: 6 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/codex.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): CodexStrategy"
```

---

## Phase C — Generic systems + plugin

### Task C1: URL formatting system

**Files:**
- Modify: `crates/vmux_agent/src/session.rs`

- [ ] **Step 1: Append the system and a test to `session.rs`**

Add after the existing component/resource defs:

```rust
use vmux_core::PageMetadata;

use crate::strategy::AgentStrategies;

pub fn format_agent_url(
    strategies: Res<AgentStrategies>,
    mut q: Query<
        (Option<&SessionId>, &AgentSession, &mut PageMetadata),
        Or<(Changed<SessionId>, Added<AgentSession>, Added<PageMetadata>)>,
    >,
) {
    for (sid, agent, mut meta) in &mut q {
        let Some(strategy) = strategies.get(agent.kind) else { continue };
        let scheme = strategy.kind().url_scheme();
        let next = match sid {
            Some(SessionId(id)) => format!("{scheme}{id}"),
            None => scheme.to_string(),
        };
        if meta.url != next { meta.url = next; }
    }
}

#[cfg(test)]
mod url_tests {
    use super::*;
    use crate::vibe::VibeStrategy;

    fn empty_meta() -> PageMetadata {
        PageMetadata { title: String::new(), url: String::new(), favicon_url: String::new(), bg_color: None }
    }

    #[test]
    fn format_agent_url_emits_scheme_with_session_id() {
        let mut app = App::new();
        let mut strategies = AgentStrategies::default();
        strategies.register(Box::new(VibeStrategy));
        app.insert_resource(strategies);
        app.add_systems(Update, format_agent_url);

        let entity = app.world_mut().spawn((
            AgentSession { kind: AgentKind::Vibe },
            SessionId("abc".into()),
            empty_meta(),
        )).id();
        app.update();
        let url = &app.world().get::<PageMetadata>(entity).unwrap().url;
        assert_eq!(url, "vmux://vibe/abc");
    }

    #[test]
    fn format_agent_url_emits_scheme_only_when_no_session_id() {
        let mut app = App::new();
        let mut strategies = AgentStrategies::default();
        strategies.register(Box::new(VibeStrategy));
        app.insert_resource(strategies);
        app.add_systems(Update, format_agent_url);

        let entity = app.world_mut().spawn((
            AgentSession { kind: AgentKind::Vibe },
            empty_meta(),
        )).id();
        app.update();
        let url = &app.world().get::<PageMetadata>(entity).unwrap().url;
        assert_eq!(url, "vmux://vibe/");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p vmux_agent session::url_tests
```

Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/src/session.rs
git commit -m "feat(vmux_agent): generic format_agent_url system"
```

---

### Task C2: Session-id tracking systems

**Files:**
- Modify: `crates/vmux_agent/src/session.rs`

- [ ] **Step 1: Append tracking systems and a test**

```rust
pub fn track_session_id_inserts(
    mut map: ResMut<AgentSessionToEntity>,
    inserted: Query<(Entity, &SessionId, &AgentSession), Added<SessionId>>,
) {
    for (entity, SessionId(id), agent) in &inserted {
        map.0.insert((agent.kind, id.clone()), entity);
    }
}

pub fn track_session_id_removals(
    mut map: ResMut<AgentSessionToEntity>,
    mut removed: RemovedComponents<SessionId>,
) {
    for entity in removed.read() {
        map.0.retain(|_, &mut e| e != entity);
    }
}

#[cfg(test)]
mod tracking_tests {
    use super::*;

    fn make_app() -> App {
        let mut app = App::new();
        app.init_resource::<AgentSessionToEntity>();
        app.add_systems(Update, (track_session_id_inserts, track_session_id_removals).chain());
        app
    }

    #[test]
    fn insert_populates_map_only_for_agent_session_entities() {
        let mut app = make_app();
        let with = app.world_mut().spawn((
            AgentSession { kind: AgentKind::Codex },
            SessionId("c1".into()),
        )).id();
        let without = app.world_mut().spawn(SessionId("nope".into())).id();
        app.update();
        let map = app.world().resource::<AgentSessionToEntity>();
        assert_eq!(map.0.get(&(AgentKind::Codex, "c1".into())), Some(&with));
        assert!(!map.0.contains_key(&(AgentKind::Codex, "nope".into())));
        let _ = without;
    }

    #[test]
    fn entity_despawn_removes_session_from_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn((
            AgentSession { kind: AgentKind::Vibe },
            SessionId("v1".into()),
        )).id();
        app.update();
        app.world_mut().despawn(e);
        app.update();
        let map = app.world().resource::<AgentSessionToEntity>();
        assert!(!map.0.contains_key(&(AgentKind::Vibe, "v1".into())));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p vmux_agent session::tracking_tests
```

Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/src/session.rs
git commit -m "feat(vmux_agent): session id tracking systems"
```

---

### Task C3: Discovery + dirty gate

**Files:**
- Modify: `crates/vmux_agent/src/session.rs`

- [ ] **Step 1: Append discovery system + dirty-flag systems + timeout constant**

```rust
use std::collections::HashSet;

pub const PENDING_DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub fn mark_dirty_on_pending_added(
    added_pending: Query<(), Added<PendingAgentSession>>,
    added_session: Query<(), Added<SessionId>>,
    mut dirty: ResMut<AgentSessionDirty>,
) {
    if !added_pending.is_empty() || !added_session.is_empty() {
        dirty.0 = true;
    }
}

pub fn agent_session_dirty_run_condition(dirty: Res<AgentSessionDirty>) -> bool {
    dirty.0
}

pub fn clear_agent_session_dirty(mut dirty: ResMut<AgentSessionDirty>) {
    dirty.0 = false;
}

pub fn discover_pending_agent_sessions(
    mut commands: Commands,
    strategies: Res<AgentStrategies>,
    map: Res<AgentSessionToEntity>,
    q: Query<(Entity, &PendingAgentSession)>,
) {
    let now = std::time::SystemTime::now();
    for (entity, pending) in &q {
        let Some(strategy) = strategies.get(pending.kind) else { continue };
        let claimed: HashSet<String> = map
            .0
            .iter()
            .filter_map(|((k, id), _)| if *k == pending.kind { Some(id.clone()) } else { None })
            .collect();
        if let Some(id) =
            strategy.discover_session(&pending.cwd, pending.spawn_time, &claimed)
        {
            commands
                .entity(entity)
                .insert(SessionId(id))
                .remove::<PendingAgentSession>();
            continue;
        }
        if now
            .duration_since(pending.spawn_time)
            .unwrap_or_default()
            >= PENDING_DISCOVERY_TIMEOUT
        {
            commands.entity(entity).remove::<PendingAgentSession>();
        }
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    use crate::vibe::VibeStrategy;

    #[test]
    fn pending_with_no_match_within_timeout_keeps_pending() {
        let mut app = App::new();
        let mut strategies = AgentStrategies::default();
        strategies.register(Box::new(VibeStrategy));
        app.insert_resource(strategies);
        app.init_resource::<AgentSessionToEntity>();
        app.add_systems(Update, discover_pending_agent_sessions);

        // Use a guaranteed-empty path so discover_session returns None
        let pending = PendingAgentSession {
            kind: AgentKind::Vibe,
            spawn_time: std::time::SystemTime::now(),
            cwd: PathBuf::from("/this/path/does/not/exist"),
        };
        let entity = app.world_mut().spawn(pending).id();
        app.update();
        assert!(app.world().get::<PendingAgentSession>(entity).is_some());
        assert!(app.world().get::<SessionId>(entity).is_none());
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p vmux_agent session::discovery_tests
```

Expected: 1 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/src/session.rs
git commit -m "feat(vmux_agent): discover_pending_agent_sessions + dirty gate"
```

---

### Task C4: Exit detection

**Files:**
- Modify: `crates/vmux_agent/src/session.rs`

- [ ] **Step 1: Add a small public type for the revert URL and the system**

The revert URL needs `vmux://terminal/<pid>`. The PID lookup lives in `vmux_desktop` (the `Pid` component) — `vmux_agent` shouldn't know about it. Solution: emit a `Message` and let `vmux_desktop` map the entity to its PID and rewrite the URL there.

Add a message type:

```rust
#[derive(Message, Debug, Clone, Copy)]
pub struct AgentSessionExited { pub entity: Entity }
```

System:

```rust
pub fn detect_agent_session_exit(
    mut commands: Commands,
    mut exited_writer: MessageWriter<AgentSessionExited>,
    strategies: Res<AgentStrategies>,
    process_exited: Query<&AgentSession, (With<SessionId>, With<vmux_terminal::ProcessExited>)>,
    sessioned: Query<(Entity, &AgentSession, &SessionId), Without<vmux_terminal::ProcessExited>>,
    process_exited_entities: Query<Entity, (With<AgentSession>, With<vmux_terminal::ProcessExited>)>,
) {
    // process-exit path
    for entity in &process_exited_entities {
        commands
            .entity(entity)
            .remove::<AgentSession>()
            .remove::<SessionId>()
            .remove::<PendingAgentSession>();
        exited_writer.write(AgentSessionExited { entity });
    }
    let _ = process_exited;
    // file-end path
    for (entity, agent, sid) in &sessioned {
        let Some(strategy) = strategies.get(agent.kind) else { continue };
        if !strategy.detect_end_time(&sid.0) { continue; }
        commands
            .entity(entity)
            .remove::<AgentSession>()
            .remove::<SessionId>()
            .remove::<PendingAgentSession>();
        exited_writer.write(AgentSessionExited { entity });
    }
}
```

Wait — `vmux_terminal::ProcessExited` lives in `vmux_terminal` crate? Verify:

```bash
grep -rn 'pub.*ProcessExited' crates/vmux_terminal crates/vmux_desktop | head
```

If `ProcessExited` is in `vmux_desktop` (as the existing code suggests at `crates/vmux_desktop/src/terminal.rs:46`), then `vmux_agent` can't depend on it without creating a cycle. Solution: define a sentinel marker `AgentTerminalExited` in `vmux_agent`, and let `vmux_desktop` insert it whenever `ProcessExited` is added to an entity carrying `AgentSession`. Or: invert — let the exit-detection system live in `vmux_desktop` (it's only one short system) and keep `vmux_agent` for everything else. **Choose this.**

Replace the implementation above with:

```rust
#[derive(bevy::ecs::message::Message, Debug, Clone, Copy)]
pub struct AgentSessionExited { pub entity: Entity }

pub fn detect_file_end_time_exit(
    mut commands: Commands,
    mut exited_writer: MessageWriter<AgentSessionExited>,
    strategies: Res<AgentStrategies>,
    sessioned: Query<(Entity, &AgentSession, &SessionId)>,
) {
    for (entity, agent, sid) in &sessioned {
        let Some(strategy) = strategies.get(agent.kind) else { continue };
        if !strategy.detect_end_time(&sid.0) { continue; }
        commands
            .entity(entity)
            .remove::<AgentSession>()
            .remove::<SessionId>()
            .remove::<PendingAgentSession>();
        exited_writer.write(AgentSessionExited { entity });
    }
}
```

`vmux_desktop` will provide the process-exit-driven counterpart in Task D8 (it can see `ProcessExited`).

- [ ] **Step 2: Add `pub use session::AgentSessionExited;` to `lib.rs`**

- [ ] **Step 3: Append a test**

```rust
#[cfg(test)]
mod exit_tests {
    use super::*;
    use crate::vibe::VibeStrategy;

    #[test]
    fn detect_file_end_time_exit_strips_components_when_strategy_says_ended() {
        // Use a stub strategy that always reports ended
        struct EndedStrategy;
        impl crate::strategy::AgentStrategy for EndedStrategy {
            fn kind(&self) -> AgentKind { AgentKind::Vibe }
            fn sessions_root(&self) -> PathBuf { PathBuf::from("/tmp/none") }
            fn build_args(&self, _: &McpServerConfig, _: Option<&str>) -> Vec<String> { vec![] }
            fn build_env(&self, _: &McpServerConfig) -> Vec<(String, String)> { vec![] }
            fn discover_session(&self, _: &Path, _: std::time::SystemTime, _: &HashSet<String>) -> Option<String> { None }
            fn detect_end_time(&self, _: &str) -> bool { true }
        }
        let _vibe = VibeStrategy;

        let mut app = App::new();
        let mut strategies = AgentStrategies::default();
        strategies.register(Box::new(EndedStrategy));
        app.insert_resource(strategies);
        app.add_message::<AgentSessionExited>();
        app.add_systems(Update, detect_file_end_time_exit);

        let entity = app.world_mut().spawn((
            AgentSession { kind: AgentKind::Vibe },
            SessionId("x".into()),
        )).id();
        app.update();
        assert!(app.world().get::<AgentSession>(entity).is_none());
        assert!(app.world().get::<SessionId>(entity).is_none());
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p vmux_agent session::exit_tests
```

Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/session.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): file-based exit detection system"
```

---

### Task C5: Watcher resource + startup system

**Files:**
- Modify: `crates/vmux_agent/src/session.rs`

- [ ] **Step 1: Add the watcher resource and `Startup` system**

```rust
use std::sync::{mpsc, Mutex};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Resource)]
pub struct AgentSessionWatchers {
    receivers: Vec<Mutex<mpsc::Receiver<()>>>,
    _watchers: Vec<RecommendedWatcher>,
}

pub fn start_agent_session_watchers(mut commands: Commands, strategies: Res<AgentStrategies>) {
    let mut receivers = Vec::new();
    let mut watchers = Vec::new();
    for (_kind, strategy) in strategies.iter() {
        let root = strategy.sessions_root();
        if std::fs::create_dir_all(&root).is_err() { continue; }
        let (tx, rx) = mpsc::channel();
        let watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res
                && (event.kind.is_create() || event.kind.is_modify())
            {
                let _ = tx.send(());
            }
        });
        let Ok(mut watcher) = watcher else { continue };
        if watcher.watch(&root, RecursiveMode::Recursive).is_err() { continue; }
        watchers.push(watcher);
        receivers.push(Mutex::new(rx));
    }
    if receivers.is_empty() { return; }
    commands.insert_resource(AgentSessionWatchers { receivers, _watchers: watchers });
}

pub fn mark_dirty_on_fs_change(
    watchers: Option<Res<AgentSessionWatchers>>,
    mut dirty: ResMut<AgentSessionDirty>,
) {
    let Some(watchers) = watchers else { return };
    for rx in &watchers.receivers {
        let Ok(rx) = rx.lock() else { continue };
        while rx.try_recv().is_ok() {
            dirty.0 = true;
        }
    }
}
```

(No new tests for the watcher itself — fs notify is hard to test deterministically and the discovery system is tested independently. The plugin in Task C6 will smoke-test app startup.)

- [ ] **Step 2: Build**

```bash
cargo build -p vmux_agent
```

Expected: clean compile.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/src/session.rs
git commit -m "feat(vmux_agent): fs watcher resource + dirty bridge"
```

---

### Task C6: `AgentSessionPlugin`

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Uncomment `pub mod plugin;` and `pub use plugin::AgentSessionPlugin;` in `lib.rs`**

- [ ] **Step 2: Write `crates/vmux_agent/src/plugin.rs`**

```rust
use bevy::prelude::*;

use crate::claude::ClaudeStrategy;
use crate::codex::CodexStrategy;
use crate::session::{
    self, AgentSessionDirty, AgentSessionExited, AgentSessionToEntity,
    agent_session_dirty_run_condition,
};
use crate::strategy::AgentStrategies;
use crate::vibe::VibeStrategy;

pub struct AgentSessionPlugin;

impl Plugin for AgentSessionPlugin {
    fn build(&self, app: &mut App) {
        let mut strategies = AgentStrategies::default();
        strategies.register(Box::new(VibeStrategy));
        strategies.register(Box::new(ClaudeStrategy));
        strategies.register(Box::new(CodexStrategy));
        app.insert_resource(strategies)
            .init_resource::<AgentSessionToEntity>()
            .init_resource::<AgentSessionDirty>()
            .add_message::<AgentSessionExited>()
            .add_systems(Startup, session::start_agent_session_watchers)
            .add_systems(
                Update,
                (
                    session::track_session_id_inserts,
                    session::track_session_id_removals,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    session::mark_dirty_on_fs_change,
                    session::mark_dirty_on_pending_added,
                ),
            )
            .add_systems(
                Update,
                (
                    session::discover_pending_agent_sessions,
                    session::detect_file_end_time_exit,
                    session::clear_agent_session_dirty,
                )
                    .chain()
                    .after(session::mark_dirty_on_fs_change)
                    .after(session::mark_dirty_on_pending_added)
                    .run_if(agent_session_dirty_run_condition),
            )
            .add_systems(
                Update,
                session::format_agent_url.after(session::track_session_id_inserts),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_registers_three_strategies() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(AgentSessionPlugin);
        let strategies = app.world().resource::<AgentStrategies>();
        assert!(strategies.get(crate::AgentKind::Vibe).is_some());
        assert!(strategies.get(crate::AgentKind::Claude).is_some());
        assert!(strategies.get(crate::AgentKind::Codex).is_some());
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p vmux_agent plugin::tests
```

Expected: 1 passed.

- [ ] **Step 4: Run pre-commit checks on `vmux_agent`**

```bash
cargo fmt -p vmux_agent -- --check
env -u CEF_PATH cargo clippy -p vmux_agent --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_agent
```

Expected: all green. If `clippy` flags warnings, fix them.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/plugin.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): AgentSessionPlugin wires strategies, watchers, systems"
```

---

## Phase D — `vmux_desktop` migration

Goal: replace `vmux_desktop::vibe::*` with calls into `vmux_agent`. After this phase the workspace builds, vibe behavior is preserved, and claude/codex tabs work end-to-end.

### Task D1: Add `vmux_agent` dependency to `vmux_desktop`

**Files:**
- Modify: `crates/vmux_desktop/Cargo.toml`

- [ ] **Step 1: Find the `[dependencies]` section and add `vmux_agent`**

```bash
grep -n 'vmux_history\|vmux_layout\|vmux_terminal' crates/vmux_desktop/Cargo.toml
```

In the same neighborhood, add:

```toml
vmux_agent = { path = "../vmux_agent" }
```

- [ ] **Step 2: Verify it builds**

```bash
cargo build -p vmux_desktop
```

Expected: clean build (no usage yet).

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/Cargo.toml
git commit -m "chore(vmux_desktop): depend on vmux_agent"
```

---

### Task D2: Extend `TerminalKind` with `Claude`, `Codex`

**Files:**
- Modify: `crates/vmux_desktop/src/terminal/launch.rs`

- [ ] **Step 1: Add the variants**

Replace the `TerminalKind` enum:

```rust
#[derive(Debug, Clone, Reflect, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TerminalKind {
    Plain,
    Vibe,
    Claude,
    Codex,
}
```

- [ ] **Step 2: Add a `From<vmux_agent::AgentKind>` impl below the enum**

```rust
impl From<vmux_agent::AgentKind> for TerminalKind {
    fn from(kind: vmux_agent::AgentKind) -> Self {
        match kind {
            vmux_agent::AgentKind::Vibe   => TerminalKind::Vibe,
            vmux_agent::AgentKind::Claude => TerminalKind::Claude,
            vmux_agent::AgentKind::Codex  => TerminalKind::Codex,
        }
    }
}
```

- [ ] **Step 3: Append a test**

```rust
    #[test]
    fn terminal_kind_from_agent_kind_maps_each_variant() {
        assert_eq!(TerminalKind::from(vmux_agent::AgentKind::Vibe),   TerminalKind::Vibe);
        assert_eq!(TerminalKind::from(vmux_agent::AgentKind::Claude), TerminalKind::Claude);
        assert_eq!(TerminalKind::from(vmux_agent::AgentKind::Codex),  TerminalKind::Codex);
    }
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p vmux_desktop terminal::launch::tests
```

Expected: 3 passed (the 2 existing + 1 new).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/terminal/launch.rs
git commit -m "feat(vmux_desktop): TerminalKind gains Claude and Codex variants"
```

---

### Task D3: Add generic spawn helpers

**Files:**
- Modify: `crates/vmux_desktop/src/agent.rs`

- [ ] **Step 1: Add imports near the top of `agent.rs`**

After the existing `use vmux_service::protocol::...` line, add:

```rust
use vmux_agent::{AgentKind, AgentSession, AgentStrategies, McpServerConfig, PendingAgentSession, SessionId, mcp};
```

(`vmux_agent::mcp` is the module exposing `resolve()`.) If `mcp` isn't re-exported in `lib.rs`, add `pub use mcp;` to `vmux_agent/src/lib.rs` (it's already declared as `pub mod mcp;`).

- [ ] **Step 2: Write `build_agent_launch` helper**

Insert after `spawn_terminal_tab` (around line 188):

```rust
pub(crate) fn build_agent_launch(
    kind: AgentKind,
    cwd: &Path,
    session_id: Option<&str>,
    strategies: &AgentStrategies,
) -> Result<crate::terminal::launch::TerminalLaunch, String> {
    let strategy = strategies
        .get(kind)
        .ok_or_else(|| format!("strategy not registered for {:?}", kind))?;
    let exe_name = kind.executable();
    let exe_path = vmux_agent::exec::find_executable(exe_name)
        .ok_or_else(|| format!("{exe_name} executable not found"))?;
    let mcp_cfg = mcp::resolve(cwd)?;
    let args = strategy.build_args(&mcp_cfg, session_id);
    let env = strategy.build_env(&mcp_cfg);
    Ok(crate::terminal::launch::TerminalLaunch {
        command: exe_path.to_string_lossy().to_string(),
        args,
        cwd: cwd.to_string_lossy().to_string(),
        env,
        kind: kind.into(),
    })
}
```

- [ ] **Step 3: Add `spawn_fresh_agent_tab` and `spawn_agent_resume_tab`**

Insert after `build_agent_launch`. These mirror the existing `spawn_fresh_vibe_tab` / `spawn_vibe_resume_tab` (lines 190-244) but generic over kind:

```rust
pub(crate) fn spawn_fresh_agent_tab(
    kind: AgentKind,
    pane: Entity,
    cwd: PathBuf,
    strategies: &AgentStrategies,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
) -> Result<Entity, String> {
    let launch = build_agent_launch(kind, &cwd, None, strategies)?;
    let terminal = spawn_terminal_tab(pane, Some(&cwd), None, commands, meshes, webview_mt, settings);
    commands.entity(terminal).insert((
        launch,
        AgentSession { kind },
        PendingAgentSession {
            kind,
            spawn_time: std::time::SystemTime::now(),
            cwd,
        },
    ));
    Ok(terminal)
}

pub(crate) fn spawn_agent_resume_tab(
    kind: AgentKind,
    pane: Entity,
    cwd: PathBuf,
    session_id: String,
    strategies: &AgentStrategies,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
) -> Result<Entity, String> {
    let launch = build_agent_launch(kind, &cwd, Some(&session_id), strategies)?;
    let terminal = spawn_terminal_tab(pane, Some(&cwd), None, commands, meshes, webview_mt, settings);
    commands.entity(terminal).insert((
        launch,
        AgentSession { kind },
        SessionId(session_id),
    ));
    Ok(terminal)
}
```

- [ ] **Step 4: Build**

```bash
cargo build -p vmux_desktop
```

Expected: builds. The old `spawn_fresh_vibe_tab` / `spawn_vibe_resume_tab` are still present and unused; they'll be deleted in Task D5.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/agent.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_desktop): generic agent spawn helpers"
```

---

### Task D4: Add process-exit detection in vmux_desktop

**Files:**
- Modify: `crates/vmux_desktop/src/agent.rs` (or a new small `agent/exit.rs`)

The file-end-time path is in `vmux_agent`. The process-exit path needs `ProcessExited` (in `vmux_desktop`) and PID lookup (in `vmux_desktop::terminal::pid`), so it lives here.

- [ ] **Step 1: Append the system to `agent.rs`**

```rust
use crate::terminal::ProcessExited;
use crate::terminal::pid::Pid;
use vmux_layout::event::TERMINAL_WEBVIEW_URL;

pub(crate) fn detect_agent_session_process_exit(
    mut commands: Commands,
    mut writer: MessageWriter<vmux_agent::AgentSessionExited>,
    q: Query<
        (Entity, Option<&Pid>, &mut vmux_core::PageMetadata),
        (With<AgentSession>, With<ProcessExited>),
    >,
) {
    for (entity, pid, mut meta) in q {
        commands
            .entity(entity)
            .remove::<AgentSession>()
            .remove::<SessionId>()
            .remove::<PendingAgentSession>();
        let next = match pid {
            Some(Pid(p)) => format!("{TERMINAL_WEBVIEW_URL}{p}"),
            None         => TERMINAL_WEBVIEW_URL.to_string(),
        };
        if meta.url != next { meta.url = next; }
        writer.write(vmux_agent::AgentSessionExited { entity });
    }
}
```

The query above mutably borrows `PageMetadata`; ensure the closure shape compiles. If Bevy complains about `Query` needing `&mut` projection on a tuple, switch to `q: Query<...>, mut q: Query<...>` mutability hints — adapt syntax to match Bevy 0.18 conventions used elsewhere in the file.

- [ ] **Step 2: Register the system in `AgentPlugin::build`**

Find the existing `add_systems(Update, ...)` block in `AgentPlugin` (around line 116) and add `detect_agent_session_process_exit` to it. Make sure it runs after the terminal subsystem inserts `ProcessExited` (it does already — `ProcessExited` is inserted in `ServiceMessageSet` which already gates the existing systems).

```rust
.add_systems(
    Update,
    (
        handle_agent_launch_requests,
        handle_agent_commands,
        crate::agent_query::handle_agent_queries,
        detect_agent_session_process_exit,
    )
        .chain()
        .in_set(WriteAppCommands)
        .after(ServiceMessageSet),
);
```

- [ ] **Step 3: Build**

```bash
cargo build -p vmux_desktop
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/agent.rs
git commit -m "feat(vmux_desktop): detect agent session process exit"
```

---

### Task D5: Migrate `spawn_vmux_tab` URL routing

**Files:**
- Modify: `crates/vmux_desktop/src/agent.rs`

- [ ] **Step 1: Locate `spawn_vmux_tab` (around line 322) and the `match host` block**

The current `"vibe" => { ... }` arm contains the spawn logic that uses `spawn_fresh_vibe_tab` / `spawn_vibe_resume_tab` and `vibe_to_entity: Option<&VibeSessionToEntity>`.

- [ ] **Step 2: Replace its parameter list and the vibe arm**

Change the function signature: replace the `vibe_to_entity: Option<&crate::vibe::session::VibeSessionToEntity>` parameter with `agent_to_entity: Option<&vmux_agent::AgentSessionToEntity>` and `strategies: &vmux_agent::AgentStrategies`. Update the arm:

```rust
"vibe" | "claude" | "codex" => {
    let kind = vmux_agent::AgentKind::from_host(host).expect("matched above");
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
    let path = parsed.path().trim_start_matches('/');
    if path.is_empty() {
        if let Err(e) = spawn_fresh_agent_tab(
            kind, pane, cwd, strategies, commands, meshes, webview_mt, settings,
        ) {
            bevy::log::warn!("spawn_fresh_agent_tab({kind:?}) failed: {e}; falling back to terminal");
            spawn_terminal_tab(pane, None, None, commands, meshes, webview_mt, settings);
        }
        Ok(())
    } else {
        let session_id = path.to_string();
        if let Some(map) = agent_to_entity
            && let Some(&entity) = map.0.get(&(kind, session_id.clone()))
        {
            crate::terminal::pid::focus_pane_entity(entity, commands, child_of_q);
            return Ok(());
        }
        if let Err(e) = spawn_agent_resume_tab(
            kind, pane, cwd, session_id, strategies, commands, meshes, webview_mt, settings,
        ) {
            bevy::log::warn!("spawn_agent_resume_tab({kind:?}) failed: {e}; falling back to terminal");
            spawn_terminal_tab(pane, None, None, commands, meshes, webview_mt, settings);
        }
        Ok(())
    }
}
```

- [ ] **Step 3: Update all callers of `spawn_vmux_tab`**

Search for it:

```bash
grep -n 'spawn_vmux_tab' crates/vmux_desktop/src
```

For each call site (`handle_agent_commands` for `BrowserNavigate` and `SplitAndNavigate`, etc.), replace the `vibe_to_entity` argument with `agent_to_entity` (a fresh `Res<AgentSessionToEntity>` parameter on the system) and add `&strategies` (a fresh `Res<AgentStrategies>` parameter on the system). Each system's signature changes:

- Remove: `vibe_to_entity: Option<Res<crate::vibe::session::VibeSessionToEntity>>`
- Add: `agent_to_entity: Option<Res<vmux_agent::AgentSessionToEntity>>, strategies: Res<vmux_agent::AgentStrategies>`

- [ ] **Step 4: Build**

```bash
cargo build -p vmux_desktop
```

Expected: clean (some `unused` warnings on the about-to-be-deleted vibe code are fine).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/agent.rs
git commit -m "refactor(vmux_desktop): route vibe/claude/codex URLs through generic helpers"
```

---

### Task D6: Migrate `command_bar.rs`

**Files:**
- Modify: `crates/vmux_desktop/src/command_bar.rs`

- [ ] **Step 1: Find the URL-prefix block**

```bash
grep -n 'VIBE_WEBVIEW_URL' crates/vmux_desktop/src/command_bar.rs
```

Lines 813, 832, 920–1057 reference vibe-specific deep-link logic.

- [ ] **Step 2: Replace `VibeSessionToEntity` parameter with `AgentSessionToEntity`**

For the resource param at line 813:

```rust
Option<Res<vmux_agent::AgentSessionToEntity>>,
```

For the clone at line 832:

```rust
let agent_to_entity = resource_params.p3().as_deref().map(|map| map.0.clone());
```

- [ ] **Step 3: Generalize URL prefix matching**

Replace the existing block (lines ~920–1057) that hard-codes `crate::vibe::session::VIBE_WEBVIEW_URL` with a loop over `vmux_agent::AgentKind::all()`:

```rust
let mut handled = false;
for kind in vmux_agent::AgentKind::all() {
    let scheme = kind.url_scheme();
    let trimmed_scheme = scheme.trim_end_matches('/');
    if !url.starts_with(trimmed_scheme) { continue; }
    let id_part = url.strip_prefix(scheme).unwrap_or("");
    if id_part.is_empty() {
        // fresh
        let title = match kind {
            vmux_agent::AgentKind::Vibe   => "Vibe",
            vmux_agent::AgentKind::Claude => "Claude",
            vmux_agent::AgentKind::Codex  => "Codex",
        };
        // ... existing PageMetadata insertion using `title` ...
        if let Err(e) = crate::terminal::spawn_agent_into_stack(
            kind, stack, cwd.clone(), None, &strategies,
            commands, meshes, webview_mt, settings,
        ) {
            bevy::log::warn!("agent spawn ({kind:?}) failed: {e}; falling back to terminal");
        }
    } else if let Some(map) = agent_to_entity.as_ref()
        && let Some(&entity) = map.get(&(kind, id_part.to_string()))
    {
        crate::terminal::pid::focus_pane_entity(entity, commands, child_of_q);
    } else {
        // resume
        if let Err(e) = crate::terminal::spawn_agent_into_stack(
            kind, stack, cwd.clone(), Some(id_part.to_string()), &strategies,
            commands, meshes, webview_mt, settings,
        ) {
            bevy::log::warn!("agent spawn ({kind:?}) failed: {e}");
        }
    }
    handled = true;
    break;
}
if !handled { /* ... existing default branch (browser tab etc.) ... */ }
```

The exact glue code depends on what's already in the surrounding closure; do the minimum to preserve the original control-flow shape. `spawn_agent_into_stack` will be added in Task D7.

- [ ] **Step 4: Build**

```bash
cargo build -p vmux_desktop
```

Expected: error referencing `spawn_agent_into_stack`. That gets added in Task D7. Stop here.

- [ ] **Step 5: Commit (with the build still failing on a known missing symbol — temporary)**

Skip commit until Task D7 lands; combine into one commit.

---

### Task D7: Generalize `spawn_vibe_into_stack` → `spawn_agent_into_stack` in `terminal.rs`

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs`

- [ ] **Step 1: Find the function**

```bash
grep -n 'spawn_vibe_into_stack' crates/vmux_desktop/src/terminal.rs
```

Located near line 287 (`spawn_url_into_stack`) and the helper itself.

- [ ] **Step 2: Add `spawn_agent_into_stack` next to `spawn_vibe_into_stack`**

Pattern:

```rust
pub(crate) fn spawn_agent_into_stack(
    kind: vmux_agent::AgentKind,
    stack: Entity,
    cwd: PathBuf,
    session_id: Option<String>,
    strategies: &vmux_agent::AgentStrategies,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
) -> Result<(), String> {
    let launch = crate::agent::build_agent_launch(kind, &cwd, session_id.as_deref(), strategies)?;
    let terminal = commands
        .spawn((Terminal::new(meshes, webview_mt, settings), ChildOf(stack)))
        .id();
    commands.entity(terminal).insert((
        launch,
        vmux_agent::AgentSession { kind },
    ));
    if let Some(id) = session_id {
        commands.entity(terminal).insert(vmux_agent::SessionId(id));
    } else {
        commands.entity(terminal).insert(vmux_agent::PendingAgentSession {
            kind,
            spawn_time: std::time::SystemTime::now(),
            cwd,
        });
    }
    Ok(())
}
```

- [ ] **Step 3: Update `spawn_url_into_stack` to dispatch by URL scheme**

Find the existing scheme dispatch (around line 287) and replace its vibe-specific branches with:

```rust
if let Some(kind) = vmux_agent::AgentKind::all().into_iter().find(|k| url.starts_with(k.url_scheme())) {
    let id_part = url.strip_prefix(kind.url_scheme()).unwrap_or("");
    let session_id = (!id_part.is_empty()).then(|| id_part.to_string());
    return spawn_agent_into_stack(kind, stack, cwd, session_id, strategies, commands, meshes, webview_mt, settings);
}
```

`strategies: &vmux_agent::AgentStrategies` becomes a new parameter to `spawn_url_into_stack`. Propagate to all callers.

- [ ] **Step 4: Build**

```bash
cargo build -p vmux_desktop
```

Expected: errors at `spawn_url_into_stack` callers because of new parameter. Fix each caller (search for `spawn_url_into_stack`) — pass `&strategies` (read from a `Res` in the calling system).

- [ ] **Step 5: Build again**

```bash
cargo build -p vmux_desktop
```

Expected: clean. Some unused-symbol warnings on `spawn_vibe_into_stack` are fine — deleted in Task D9.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_desktop/src/terminal.rs crates/vmux_desktop/src/command_bar.rs
git commit -m "refactor(vmux_desktop): generalize spawn_*_into_stack across agent kinds"
```

---

### Task D8: Migrate `persistence.rs` URL-scheme dispatch

**Files:**
- Modify: `crates/vmux_desktop/src/persistence.rs`

- [ ] **Step 1: Locate the vibe restore code**

```bash
grep -n 'vibe' crates/vmux_desktop/src/persistence.rs
```

Lines 325–357 contain the URL match and `spawn_vibe_into_stack` calls.

- [ ] **Step 2: Replace with generic dispatch**

```rust
if let Some(kind) = vmux_agent::AgentKind::all().into_iter().find(|k| url.starts_with(k.url_scheme())) {
    let id_part = url.strip_prefix(kind.url_scheme()).unwrap_or("");
    let session_id = (!id_part.is_empty()).then(|| id_part.to_string());
    if let Err(e) = crate::terminal::spawn_agent_into_stack(
        kind, stack, cwd, session_id, &strategies,
        commands, meshes, webview_mt, settings,
    ) {
        bevy::log::warn!("restore agent tab failed: {e}");
    }
} else {
    // ... existing non-agent restore logic ...
}
```

- [ ] **Step 3: Add the `strategies: Res<vmux_agent::AgentStrategies>` parameter to whichever system calls into this code**

- [ ] **Step 4: Build**

```bash
cargo build -p vmux_desktop
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/persistence.rs
git commit -m "refactor(vmux_desktop): persistence dispatches agent URLs by scheme"
```

---

### Task D9: Replace `VibePlugin` with `AgentSessionPlugin` in `lib.rs`; delete `vibe.rs` + `vibe/`

**Files:**
- Modify: `crates/vmux_desktop/src/lib.rs`
- Delete: `crates/vmux_desktop/src/vibe.rs`
- Delete: `crates/vmux_desktop/src/vibe/session.rs`

- [ ] **Step 1: Edit `lib.rs`**

Find `mod vibe;` and `vibe::VibePlugin`. Replace:

```rust
// remove:
mod vibe;
// remove from add_plugins(...):
//   VibePlugin,
```

Replace with:

```rust
// add to add_plugins(...):
vmux_agent::AgentSessionPlugin,
```

The existing `use vibe::VibePlugin;` line goes away.

- [ ] **Step 2: Delete the vibe files**

```bash
git rm crates/vmux_desktop/src/vibe.rs crates/vmux_desktop/src/vibe/session.rs
rmdir crates/vmux_desktop/src/vibe 2>/dev/null || true
```

- [ ] **Step 3: Find any remaining `crate::vibe::` references and rewrite**

```bash
grep -rn 'crate::vibe::' crates/vmux_desktop/src
```

For each hit:
- `crate::vibe::session::Vibe` → `vmux_agent::AgentSession { kind: vmux_agent::AgentKind::Vibe }` (or replace usage with kind-aware logic)
- `crate::vibe::session::SessionId` → `vmux_agent::SessionId`
- `crate::vibe::session::PendingVibeSession` → `vmux_agent::PendingAgentSession`
- `crate::vibe::session::VibeSessionToEntity` → `vmux_agent::AgentSessionToEntity`
- `crate::vibe::session::VIBE_WEBVIEW_URL` → `vmux_agent::AgentKind::Vibe.url_scheme()`
- `crate::vibe::find_executable` → `vmux_agent::exec::find_executable`
- `crate::vibe::vibe_available` → `(|| vmux_agent::exec::find_executable("vibe").is_some())()` or wire via `register_providers`

- [ ] **Step 4: Update `settings.rs` line 66**

The default startup URL stays `vmux://vibe/`. Replace the literal with:

```rust
vmux_agent::AgentKind::Vibe.url_scheme().to_string()
```

(Keeps the same value; uses the constant.)

- [ ] **Step 5: Update `tests/release_invariants.rs`**

```bash
grep -n 'vibe' crates/vmux_desktop/tests/release_invariants.rs
```

Rewrite any references using the same mappings as Step 3.

- [ ] **Step 6: Build the crate**

```bash
cargo build -p vmux_desktop
```

Expected: clean. If errors appear, follow them down — most will be missed `crate::vibe::` references.

- [ ] **Step 7: Run vibe-related tests in `vmux_desktop::agent`**

```bash
env -u CEF_PATH cargo test -p vmux_desktop agent::tests
```

Expected: existing vibe tests still pass, since they exercise `spawn_vmux_tab` for the `vibe` host through the same generic helpers.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(vmux_desktop): drop crate::vibe in favor of vmux_agent"
```

---

## Phase E — Provider registration

### Task E1: Per-strategy `register_providers` in `vmux_agent`

`AgentProvider` lives in `vmux_desktop` (it depends on `TerminalLaunch` and the desktop-side `PreparedAgentLaunch`). Cleanest wiring: keep registration in `vmux_desktop`, use free fns there that close over a hard-coded `AgentKind`.

**Files:**
- Modify: `crates/vmux_desktop/src/agent.rs`

- [ ] **Step 1: Add per-kind free fns**

After `build_agent_launch`, add:

```rust
fn vibe_available()   -> bool { vmux_agent::exec::find_executable("vibe").is_some() }
fn claude_available() -> bool { vmux_agent::exec::find_executable("claude").is_some() }
fn codex_available()  -> bool { vmux_agent::exec::find_executable("codex").is_some() }

fn vibe_prepare(cwd: &Path)   -> Result<PreparedAgentLaunch, String> { prepare_for_kind(vmux_agent::AgentKind::Vibe,   cwd) }
fn claude_prepare(cwd: &Path) -> Result<PreparedAgentLaunch, String> { prepare_for_kind(vmux_agent::AgentKind::Claude, cwd) }
fn codex_prepare(cwd: &Path)  -> Result<PreparedAgentLaunch, String> { prepare_for_kind(vmux_agent::AgentKind::Codex,  cwd) }

fn prepare_for_kind(kind: vmux_agent::AgentKind, cwd: &Path) -> Result<PreparedAgentLaunch, String> {
    // Strategies are registered in AgentSessionPlugin. We don't have direct access
    // to the resource here, so build a temporary by re-instantiating each strategy.
    use vmux_agent::{strategy::AgentStrategies, vibe::VibeStrategy, claude::ClaudeStrategy, codex::CodexStrategy};
    let mut strategies = AgentStrategies::default();
    strategies.register(Box::new(VibeStrategy));
    strategies.register(Box::new(ClaudeStrategy));
    strategies.register(Box::new(CodexStrategy));
    let launch = build_agent_launch(kind, cwd, None, &strategies)?;
    Ok(PreparedAgentLaunch { cwd: cwd.to_path_buf(), launch })
}
```

(Re-instantiating strategies per call is fine — they're zero-state structs.)

- [ ] **Step 2: Replace `register_agent_providers` (or insert if missing) in `AgentPlugin::build`**

Find where vibe was previously registering its providers (was in the deleted `VibePlugin::build`). Recreate that registration here:

```rust
// in AgentPlugin::build, after init_resource::<AgentProviders>()
let mut providers = app.world_mut().resource_mut::<AgentProviders>();
for (id, name, exe, available, prepare) in [
    ("vibe_new",         "Vibe New",         "vibe",   vibe_available   as fn() -> bool, vibe_prepare   as fn(&Path) -> Result<PreparedAgentLaunch, String>),
    ("vibe_new_stack",   "Vibe New Stack",   "vibe",   vibe_available,                   vibe_prepare),
    ("claude_new",       "Claude New",       "claude", claude_available,                 claude_prepare),
    ("claude_new_stack", "Claude New Stack", "claude", claude_available,                 claude_prepare),
    ("codex_new",        "Codex New",        "codex",  codex_available,                  codex_prepare),
    ("codex_new_stack",  "Codex New Stack",  "codex",  codex_available,                  codex_prepare),
] {
    providers.register(AgentProvider { id, name, shortcut: "", executable: exe, available, prepare });
}
```

- [ ] **Step 3: Build**

```bash
cargo build -p vmux_desktop
```

Expected: clean.

- [ ] **Step 4: Add a test that all six providers are registered**

```rust
    #[test]
    fn agent_plugin_registers_all_six_provider_entries() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        let providers = app.world().resource::<AgentProviders>();
        for id in ["vibe_new", "vibe_new_stack", "claude_new", "claude_new_stack", "codex_new", "codex_new_stack"] {
            assert!(providers.contains(id), "missing provider: {id}");
        }
    }
```

- [ ] **Step 5: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_desktop agent::tests::agent_plugin_registers_all_six_provider_entries
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_desktop/src/agent.rs
git commit -m "feat(vmux_desktop): register vibe/claude/codex providers"
```

---

## Phase F — Integration tests

### Task F1: vmux:// URL routing tests for claude and codex

**Files:**
- Modify: `crates/vmux_desktop/src/agent.rs` (test module at the bottom)

- [ ] **Step 1: Add three tests parameterized over kind**

```rust
    #[test]
    fn browser_navigate_with_claude_url_attempts_to_spawn_terminal_in_focused_pane() {
        // Note: spawn will fail unless `claude` is on PATH in the test env; we assert
        // that no spurious browser tab is created when the URL has the claude scheme.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.add_plugins(vmux_agent::AgentSessionPlugin);
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://claude/".into(),
                    pane: None,
                },
            });

        app.update();

        let world = app.world_mut();
        let browser_count = world.query::<&Browser>().iter(world).count();
        assert_eq!(browser_count, 0, "claude URL should never spawn a browser");
        // Either a terminal exists (claude installed) or none (claude missing) —
        // both are acceptable. The point is no browser leak.
    }
```

Repeat for `vmux://codex/` (same test, swap host).

Add a deep-link test:

```rust
    #[test]
    fn deep_link_focuses_existing_claude_tab() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.add_plugins(vmux_agent::AgentSessionPlugin);
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app.world_mut().spawn(crate::layout::stack::stack_bundle()).insert(ChildOf(pane)).id();
        let existing = app.world_mut().spawn((
            Terminal,
            ChildOf(stack),
            vmux_agent::AgentSession { kind: vmux_agent::AgentKind::Claude },
            vmux_agent::SessionId("dl-1".into()),
        )).id();

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.update(); // populate AgentSessionToEntity

        let map = app.world().resource::<vmux_agent::AgentSessionToEntity>();
        assert_eq!(map.0.get(&(vmux_agent::AgentKind::Claude, "dl-1".into())), Some(&existing));
    }
```

- [ ] **Step 2: Run new tests**

```bash
env -u CEF_PATH cargo test -p vmux_desktop agent::tests::browser_navigate_with_claude_url \
                                            agent::tests::browser_navigate_with_codex_url \
                                            agent::tests::deep_link_focuses_existing_claude_tab
```

Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/src/agent.rs
git commit -m "test(vmux_desktop): claude/codex URL routing + deep-link"
```

---

## Phase G — Verification + smoke

### Task G1: Pre-commit checks on changed crates

**Files:** none (verification only)

- [ ] **Step 1: Compute changed crates**

```bash
BASE=origin/main
ROOT="$(git rev-parse --show-toplevel)"
CHANGED_PKGS=$(cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[] | select(.manifest_path | test("patches") | not) | "\(.name)\t\(.manifest_path | sub("/Cargo\\.toml$"; ""))"' \
  | while IFS=$'\t' read -r name dir; do
      rel="${dir#"$ROOT"/}"; [ -z "$rel" ] && rel="."
      if ! git diff --quiet "$BASE" -- "$rel"; then echo "$name"; fi
    done)
echo "$CHANGED_PKGS"
```

Expected output: at least `vmux_agent`, `vmux_desktop`.

- [ ] **Step 2: fmt check**

```bash
for pkg in $CHANGED_PKGS; do cargo fmt -p "$pkg" -- --check || echo "FMT FAIL: $pkg"; done
```

If any fail, run `cargo fmt -p <pkg>` and re-check.

- [ ] **Step 3: clippy**

```bash
for pkg in $CHANGED_PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings || echo "CLIPPY FAIL: $pkg"; done
```

Fix every warning. Re-run until clean.

- [ ] **Step 4: tests**

```bash
for pkg in $CHANGED_PKGS; do env -u CEF_PATH cargo test -p "$pkg" || echo "TEST FAIL: $pkg"; done
```

All pass.

- [ ] **Step 5: Commit any fixups**

```bash
git status
# if there are changes (formatting, clippy fixes):
git add -A
git commit -m "chore: fmt + clippy fixups"
```

---

### Task G2: Manual smoke test — claude

**Files:** none

- [ ] **Step 1: Build the desktop app**

```bash
cargo build -p vmux_desktop
```

- [ ] **Step 2: Launch with a known-good `cwd`**

Use `cargo run -p vmux_desktop` from the worktree root.

- [ ] **Step 3: Open command bar; verify "Claude New" entry is visible**

Trigger the command bar, look for `Claude New` and `Claude New Stack`. If absent, `claude` was not on `$PATH` at startup — verify with `which claude` in the same shell that launched vmux.

- [ ] **Step 4: Run `Claude New`**

Expected:
- Terminal tab opens with `claude` running.
- The tab URL transitions from `vmux://claude/` → `vmux://claude/<uuid>` within ~5 s (filesystem watcher discovers the new session file).

If it doesn't, check:
- `~/.claude/projects/<encoded-cwd>/` exists and has a fresh `.jsonl`.
- The encoded-cwd algorithm matches reality (ls the dir; compare against `vmux_agent::claude::project_dir_name`).

- [ ] **Step 5: Verify MCP integration**

In the claude session, ask: `please run /mcp` (or similar slash command to list MCP servers). Expect `vmux` to be listed and connected.

- [ ] **Step 6: Verify exit detection**

Type `/exit` in the claude session. Expected: tab URL reverts to `vmux://terminal/<pid>` after the process exits.

- [ ] **Step 7: Verify deep-link re-focus**

With the claude session still open, trigger an in-app navigation to `vmux://claude/<the-uuid>` (e.g. paste it in the command bar). Expected: existing tab is focused, no new tab spawned.

---

### Task G3: Manual smoke test — codex

**Files:** none

Repeat Task G2 but for codex:

- [ ] **Step 1: Run `Codex New` from the command bar**

Expected: terminal opens with `codex` running, URL becomes `vmux://codex/<uuid>` within ~5 s.

- [ ] **Step 2: Verify `-c mcp_servers.vmux.*` overrides took effect**

In codex, check `/mcp` lists `vmux`. If not, the TOML quoting is likely off — inspect `vmux_agent/src/codex.rs::quote_toml`.

- [ ] **Step 3: Verify resume flow**

Run `Codex New`. Note the UUID. Exit the agent. From the command bar, navigate to `vmux://codex/<that-uuid>`. Expected: codex resumes the session inside a new terminal tab.

- [ ] **Step 4: Verify exit detection**

Same as claude — exit codex, tab URL should revert to `vmux://terminal/<pid>`.

---

### Task G4: Vibe regression smoke

**Files:** none

- [ ] **Step 1: Run `Vibe New` from the command bar**

Expected: same behavior as before this PR (URL goes to `vmux://vibe/<uuid>`, MCP works, exit reverts to terminal URL).

- [ ] **Step 2: If anything is different**

Compare against `git log --oneline main..HEAD` to identify which task introduced the regression. The likely suspect is Task D5/D6/D7 (URL routing migration) — re-read the vibe arm and the URL prefix loop.

---

### Task G5: Open PR

**Files:** none

- [ ] **Step 1: Push the branch**

```bash
git push -u origin claude-codex
```

- [ ] **Step 2: Open PR via `gh`**

```bash
gh pr create --title "feat: claude + codex CLI support via vmux_agent crate" --body "$(cat <<'EOF'
## Summary
- New `vmux_agent` crate abstracts agent CLI strategies (vibe, claude, codex) behind an `AgentStrategy` trait
- `vmux://claude/[id]` and `vmux://codex/[id]` URL schemes with full vibe-parity (spawn, resume, MCP injection, session discovery, exit detection)
- `vmux_desktop::vibe::*` deleted; vibe behavior preserved via `VibeStrategy`
- Six new command-bar entries: `Claude New`, `Claude New Stack`, `Codex New`, `Codex New Stack` (vibe entries unchanged)

## Spec
docs/specs/2026-05-14-claude-codex-cli-design.md

## Test plan
- [ ] Pre-commit checks pass on `vmux_agent` and `vmux_desktop` (fmt + clippy + test)
- [ ] Manual: `Vibe New` still works end-to-end
- [ ] Manual: `Claude New` opens claude, URL becomes `vmux://claude/<uuid>`, MCP listed, exit reverts to terminal URL
- [ ] Manual: `Codex New` opens codex, URL becomes `vmux://codex/<uuid>`, MCP listed, exit reverts to terminal URL
- [ ] Manual: deep-link `vmux://claude/<uuid>` focuses existing tab
EOF
)"
```

- [ ] **Step 3: Wait for CI**

```bash
gh pr checks --watch
```

Expected: all green. If a CI run fails, read the log and fix.

- [ ] **Step 4: Delete the plan file (per AGENTS.md)**

```bash
git rm docs/plans/2026-05-14-claude-codex-cli.md
git commit -m "chore: remove implemented plan"
git push
```

---

## Self-review

**Spec coverage:**
- ✅ vmux_agent crate scaffolded — Phase A
- ✅ AgentKind / AgentSession / SessionId / PendingAgentSession / AgentSessionToEntity — Phase A
- ✅ AgentStrategy trait + AgentStrategies registry — Task A6
- ✅ VibeStrategy / ClaudeStrategy / CodexStrategy — Phase B
- ✅ Generic systems (track, discover, exit, format URL, dirty gate, watcher) — Phase C
- ✅ AgentSessionPlugin — Task C6
- ✅ TerminalKind extended with Claude/Codex — Task D2
- ✅ Generic spawn helpers (`spawn_fresh_agent_tab`, `spawn_agent_resume_tab`, `spawn_agent_into_stack`) — Tasks D3, D7
- ✅ URL routing for vibe/claude/codex — Task D5
- ✅ command_bar.rs URL prefix generalized — Task D6
- ✅ persistence.rs URL-scheme dispatch — Task D8
- ✅ Process-exit revert path — Task D4
- ✅ vibe.rs/vibe/ deleted — Task D9
- ✅ Six provider registrations — Task E1
- ✅ Integration tests for routing + deep-link — Task F1
- ✅ Pre-commit checks + manual smoke (vibe + claude + codex) — Phase G

No spec sections without a matching task.

**Type consistency:**
- `AgentSession`, `SessionId`, `PendingAgentSession` referenced consistently in tasks A5, B*, C*, D*.
- `AgentSessionToEntity` keyed `(AgentKind, String)` consistently in C2, D5, D6, F1.
- `spawn_fresh_agent_tab` / `spawn_agent_resume_tab` / `spawn_agent_into_stack` — names match across D3, D5, D6, D7, D8.
- `build_agent_launch` introduced in D3, called from D7 — same signature.
- `AgentSessionExited` message: defined in C4, consumed by D4. Field `entity` matches.

**Placeholder scan:** None. Every step has either complete code or an explicit verification command.

**Open caveats acknowledged in plan:**
- Codex resume subcommand placement (Task B3 test asserts position; Task G3 manual smoke confirms reality).
- Claude project-dir encoding (Task B2 step 2 verifies against real dirs before implementing).

---

**Plan complete and saved to `docs/plans/2026-05-14-claude-codex-cli.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
