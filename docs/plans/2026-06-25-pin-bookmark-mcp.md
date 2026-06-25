# Pins + Bookmarks — MCP Control Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Expose the bookmarks feature to agents/MCP: `bookmark_list`, `bookmark_add`, `bookmark_remove`, `bookmark_pin`, `bookmark_unpin`, `bookmark_folder_create`.

**Prerequisite:** The interactive plan (`2026-06-25-pin-bookmark.md`) is implemented — specifically `vmux_layout::bookmark::BookmarkOp` and the ECS components (`Pin`/`Bookmark`/`Folder`/`Uuid`/`PageMetadata`/`Name`/`Order`/`Collapsed`).

**Architecture:** A query (`BookmarkList`) reads the ECS world and returns a JSON snapshot in one response (vibe MCP needs one-shot). Commands map to a single `AgentCommand::BookmarkCommand { command, .. }` discriminator variant (mirroring `SpaceCommand`); the agent handler writes `BookmarkOp` via a `MessageWriter`. `vmux_agent` already depends on `vmux_layout` (no cycle).

**Tech Stack:** Rust, vmux MCP stdio server, rkyv service protocol, Bevy messages.

**Files touched:** `vmux_service/src/protocol.rs`, `vmux_mcp/src/tools.rs`, `vmux_agent/src/plugin.rs`, `vmux_cli/tests/mcp_smoke.rs`. (No `AgentQueryResult` variant added — reuse `Spaces(String)` for the list JSON, so `vmux_mcp/src/protocol.rs` + `vmux_service/src/server.rs` need no edits.)

---

## Phase 1 — Protocol (`vmux_service/src/protocol.rs`)

### Task 1: Add `AgentQuery::BookmarkList` + `AgentCommand::BookmarkCommand`

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs` (`AgentQuery` ~line 163; `AgentCommand` ~line 55; `validate_agent_command` ~line 238)

- [ ] **Step 1: Add the query variant**

In `AgentQuery` (~line 163), add:

```rust
    BookmarkList,
```

- [ ] **Step 2: Add the command variant**

In `AgentCommand` (~line 55), add (mirror `SpaceCommand` at line 106):

```rust
    BookmarkCommand {
        command: String,
        uuid: Option<String>,
        name: Option<String>,
        url: Option<String>,
        title: Option<String>,
        favicon_url: Option<String>,
    },
```

- [ ] **Step 3: Add a validation arm**

In `validate_agent_command` (~line 238), add an arm that rejects an empty `command` (mirror the `SpaceCommand` validation):

```rust
        AgentCommand::BookmarkCommand { command, .. } if command.trim().is_empty() => {
            Err("bookmark command must not be empty".to_string())
        }
```

(Place it consistent with the existing match structure; if the fn returns `Ok(())` by default, only add the guard arm.)

- [ ] **Step 4: Build**

Run: `cargo build -p vmux_service`
Expected: compiles (downstream non-exhaustive matches may warn/err — fixed in later tasks; build the whole workspace at the end).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_service/src/protocol.rs
git commit -m "feat(service): add BookmarkList query + BookmarkCommand"
```

---

## Phase 2 — MCP tool definitions + dispatch (`vmux_mcp/src/tools.rs`)

### Task 2: Tool definitions

**Files:**
- Modify: `crates/vmux_mcp/src/tools.rs` (add `*_definition()` fns near `list_spaces_definition`/`open_page_definition` ~line 307-360; push them in `tool_definitions()` ~line 507)

- [ ] **Step 1: Add definition functions**

```rust
fn bookmark_list_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_list".into(),
        description: "List all pins (favicon quick-access) and bookmarks (saved pages, \
optionally inside folders) for the current profile. Returns JSON: \
{pins:[{uuid,url,title,favicon_url}], roots:[ entry | folder{uuid,name,collapsed,children:[entry]} ]}."
            .into(),
        input_schema: serde_json::json!({"type":"object","properties":{},"additionalProperties":false}),
    }
}

fn bookmark_add_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_add".into(),
        description: "Save a page as a bookmark. Optional folder (a folder uuid from bookmark_list) \
nests it; omit for top level."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["url"],
            "additionalProperties": false,
            "properties": {
                "url": {"type": "string"},
                "title": {"type": "string"},
                "favicon_url": {"type": "string"},
                "folder": {"type": "string"}
            }
        }),
    }
}

fn bookmark_remove_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_remove".into(),
        description: "Remove a bookmark by its uuid (from bookmark_list).".into(),
        input_schema: serde_json::json!({
            "type":"object","required":["uuid"],"additionalProperties":false,
            "properties":{"uuid":{"type":"string"}}
        }),
    }
}

fn bookmark_pin_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_pin".into(),
        description: "Pin a page to the favicon grid. Provide a bookmark uuid to promote an \
existing bookmark, OR a url (+optional title/favicon_url) to pin a page directly."
            .into(),
        input_schema: serde_json::json!({
            "type":"object","additionalProperties":false,
            "properties":{
                "uuid":{"type":"string"},
                "url":{"type":"string"},
                "title":{"type":"string"},
                "favicon_url":{"type":"string"}
            }
        }),
    }
}

fn bookmark_unpin_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_unpin".into(),
        description: "Unpin a pin by its uuid (from bookmark_list).".into(),
        input_schema: serde_json::json!({
            "type":"object","required":["uuid"],"additionalProperties":false,
            "properties":{"uuid":{"type":"string"}}
        }),
    }
}

fn bookmark_folder_create_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_folder_create".into(),
        description: "Create a bookmark folder with the given name.".into(),
        input_schema: serde_json::json!({
            "type":"object","required":["name"],"additionalProperties":false,
            "properties":{"name":{"type":"string"}}
        }),
    }
}
```

- [ ] **Step 2: Push them in `tool_definitions()`**

After `defs.push(record_stop_definition());` (~line 521):

```rust
    defs.push(bookmark_list_definition());
    defs.push(bookmark_add_definition());
    defs.push(bookmark_remove_definition());
    defs.push(bookmark_pin_definition());
    defs.push(bookmark_unpin_definition());
    defs.push(bookmark_folder_create_definition());
```

- [ ] **Step 3: Build**

Run: `cargo build -p vmux_mcp`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_mcp/src/tools.rs
git commit -m "feat(mcp): bookmark tool definitions"
```

### Task 3: Dispatch + unit tests

**Files:**
- Modify: `crates/vmux_mcp/src/tools.rs` (dispatch branches in `dispatch_with_anchor` BEFORE the macro fallbacks ~line 753-758; tests in `mod tests` ~line 779)

- [ ] **Step 1: Write failing dispatch tests**

Add to `mod tests` in `tools.rs` (mirror `list_spaces_dispatches_to_query` / `rename_space_dispatches_to_space_command`):

```rust
    #[test]
    fn bookmark_list_dispatches_to_query() {
        let target = dispatch_from_tool_call("bookmark_list", serde_json::json!({})).unwrap();
        assert!(matches!(
            target,
            DispatchTarget::Query(vmux_service::protocol::AgentQuery::BookmarkList)
        ));
    }

    #[test]
    fn bookmark_add_dispatches_to_command() {
        let cmd = dispatch_command(
            "bookmark_add",
            serde_json::json!({"url": "https://a.test", "title": "A"}),
        )
        .unwrap();
        match cmd {
            AgentCommand::BookmarkCommand { command, url, title, .. } => {
                assert_eq!(command, "add");
                assert_eq!(url.as_deref(), Some("https://a.test"));
                assert_eq!(title.as_deref(), Some("A"));
            }
            other => panic!("expected BookmarkCommand, got {other:?}"),
        }
    }

    #[test]
    fn bookmark_folder_create_dispatches_to_command() {
        let cmd = dispatch_command("bookmark_folder_create", serde_json::json!({"name": "PRs"}))
            .unwrap();
        match cmd {
            AgentCommand::BookmarkCommand { command, name, .. } => {
                assert_eq!(command, "folder_create");
                assert_eq!(name.as_deref(), Some("PRs"));
            }
            other => panic!("expected BookmarkCommand, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p vmux_mcp bookmark_`
Expected: FAIL (`unknown tool`).

- [ ] **Step 3: Add dispatch branches**

In `dispatch_with_anchor`, BEFORE the `McpParamTool::from_mcp_call` fallback (~line 758), add:

```rust
    if name == "bookmark_list" {
        return Ok(DispatchTarget::Query(
            vmux_service::protocol::AgentQuery::BookmarkList,
        ));
    }
    let bookmark_cmd = |command: &str| -> Result<DispatchTarget, String> {
        Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand {
            command: command.to_string(),
            uuid: arguments.get("uuid").and_then(Value::as_str).map(str::to_string),
            name: arguments.get("name").and_then(Value::as_str).map(str::to_string),
            url: arguments.get("url").and_then(Value::as_str).map(str::to_string),
            title: arguments.get("title").and_then(Value::as_str).map(str::to_string),
            favicon_url: arguments
                .get("favicon_url")
                .and_then(Value::as_str)
                .map(str::to_string),
        }))
    };
    match name {
        "bookmark_add" => {
            if arguments.get("url").and_then(Value::as_str).unwrap_or("").is_empty() {
                return Err("bookmark_add.url is required".to_string());
            }
            return bookmark_cmd("add");
        }
        "bookmark_remove" => return bookmark_cmd("remove"),
        "bookmark_pin" => return bookmark_cmd("pin"),
        "bookmark_unpin" => return bookmark_cmd("unpin"),
        "bookmark_folder_create" => {
            if arguments.get("name").and_then(Value::as_str).unwrap_or("").is_empty() {
                return Err("bookmark_folder_create.name is required".to_string());
            }
            return bookmark_cmd("folder_create");
        }
        _ => {}
    }
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p vmux_mcp bookmark_`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_mcp/src/tools.rs
git commit -m "feat(mcp): dispatch bookmark tools to query/command"
```

---

## Phase 3 — Agent handlers (`vmux_agent/src/plugin.rs`)

### Task 4: Query handler — `BookmarkList` → JSON snapshot

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs` (`handle_agent_queries` ~line 1161; add ECS queries to the system params; add the `BookmarkList` arm near `ListSpaces` ~line 1198)

- [ ] **Step 1: Add the handler arm**

Add the necessary `Query`s to `handle_agent_queries`'s params (a `SystemParam` struct or direct params — match the existing style):

```rust
    bm_pins: Query<(&vmux_core::Uuid, &vmux_core::PageMetadata), With<vmux_core::Pin>>,
    bm_folders: Query<
        (Entity, &vmux_core::Uuid, &Name, Option<&Children>, Has<vmux_core::Collapsed>, &vmux_core::Order),
        With<vmux_core::Folder>,
    >,
    bm_top: Query<
        (&vmux_core::Uuid, &vmux_core::PageMetadata, &vmux_core::Order),
        (With<vmux_core::Bookmark>, Without<vmux_core::Pin>, Without<ChildOf>),
    >,
    bm_children: Query<
        (&vmux_core::Uuid, &vmux_core::PageMetadata),
        (With<vmux_core::Bookmark>, Without<vmux_core::Pin>),
    >,
```

Arm (mirror `ListSpaces` at `plugin.rs:1198` — build JSON, send `AgentQueryResult::Spaces(json)` to reuse the existing render arm):

```rust
            AgentQuery::BookmarkList => {
                let row = |u: &vmux_core::Uuid, m: &vmux_core::PageMetadata| serde_json::json!({
                    "uuid": u.0, "url": m.url, "title": m.title, "favicon_url": m.favicon_url,
                });
                let pins: Vec<_> = bm_pins.iter().map(|(u, m)| row(u, m)).collect();
                let mut roots: Vec<(u32, serde_json::Value)> = Vec::new();
                for (entity, uuid, name, children, collapsed, order) in bm_folders.iter() {
                    let _ = entity;
                    let mut kids: Vec<serde_json::Value> = Vec::new();
                    if let Some(children) = children {
                        for child in children.iter() {
                            if let Ok((u, m)) = bm_children.get(child) {
                                kids.push(row(u, m));
                            }
                        }
                    }
                    roots.push((order.0, serde_json::json!({
                        "kind": "folder", "uuid": uuid.0, "name": name.as_str(),
                        "collapsed": collapsed, "children": kids,
                    })));
                }
                for (uuid, meta, order) in bm_top.iter() {
                    let mut entry = row(uuid, meta);
                    entry["kind"] = serde_json::json!("entry");
                    roots.push((order.0, entry));
                }
                roots.sort_by_key(|(o, _)| *o);
                let roots: Vec<_> = roots.into_iter().map(|(_, v)| v).collect();
                let json = serde_json::to_string(&serde_json::json!({"pins": pins, "roots": roots}))
                    .unwrap_or_else(|_| "{}".to_string());
                service.0.send(ClientMessage::AgentQueryResponse {
                    request_id: request.request_id,
                    result: AgentQueryResult::Spaces(json),
                });
            }
```

> NOTE: match the exact field names the surrounding code uses (`service`, `request.request_id`, `ClientMessage::AgentQueryResponse`) — copy from the `ListSpaces`/`GetSettings` arms verbatim. Add `Has`, `ChildOf`, `Children`, `Name` imports if missing.

- [ ] **Step 2: Build**

Run: `cargo build -p vmux_agent`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/src/plugin.rs
git commit -m "feat(agent): bookmark_list query handler"
```

### Task 5: Command handler — `BookmarkCommand` → `BookmarkOp`

**Files:**
- Modify: `crates/vmux_agent/src/plugin.rs` (the command match ~line 752, near the `SpaceCommand` arm; add a `MessageWriter<vmux_layout::bookmark::BookmarkOp>` to the writers)

- [ ] **Step 1: Add the writer + arm**

Add to the command-handling system's writers (mirror how `writers.space_command` is provided): a `MessageWriter<vmux_layout::bookmark::BookmarkOp>` (call it `bookmark_op`). Add the arm:

```rust
            ServiceAgentCommand::BookmarkCommand { command, uuid, name, url, title, favicon_url } => {
                use vmux_layout::bookmark::BookmarkOp;
                let op = match command.as_str() {
                    "add" => url.clone().map(|url| BookmarkOp::Add {
                        url,
                        title: title.clone().unwrap_or_default(),
                        favicon_url: favicon_url.clone().unwrap_or_default(),
                        folder: uuid.clone(),
                    }),
                    "remove" => uuid.clone().map(|uuid| BookmarkOp::Remove { uuid }),
                    "pin" => match (uuid.clone(), url.clone()) {
                        (Some(uuid), _) => Some(BookmarkOp::Pin { uuid }),
                        (None, Some(url)) => Some(BookmarkOp::PinUrl {
                            url,
                            title: title.clone().unwrap_or_default(),
                            favicon_url: favicon_url.clone().unwrap_or_default(),
                        }),
                        _ => None,
                    },
                    "unpin" => uuid.clone().map(|uuid| BookmarkOp::Unpin { uuid }),
                    "folder_create" => name.clone().map(|name| BookmarkOp::AddFolder { name }),
                    _ => None,
                };
                match op {
                    Some(op) => {
                        writers.bookmark_op.write(op);
                        AgentCommandResult::Ok
                    }
                    None => AgentCommandResult::Error("invalid bookmark command".to_string()),
                }
            }
```

> NOTE: the local `ServiceAgentCommand` is the protocol `AgentCommand` (check the alias used in this match; copy from the `SpaceCommand` arm at `plugin.rs:752`). `add` reuses `uuid` as the optional folder id (the tool passes `folder` → dispatch maps it into `uuid`? No — re-check: in Plan-2 Task 3, `bookmark_add` reads `folder` into... ). **Fix:** in Task 3, also read `folder` into the command's `uuid` field OR add a `folder` field. Simpler: in the `bookmark_add` dispatch branch, set `uuid` from `arguments["folder"]` for the add case. Update the `bookmark_cmd("add")` call to override `uuid` with the `folder` arg. See Task 3 amendment below.)

- [ ] **Step 2: Amend Task 3 dispatch for `bookmark_add` folder**

Ensure the `add` branch carries the folder id. Replace the `"bookmark_add"` arm to build the command with `uuid` sourced from `folder`:

```rust
        "bookmark_add" => {
            let url = arguments.get("url").and_then(Value::as_str).unwrap_or("");
            if url.is_empty() {
                return Err("bookmark_add.url is required".to_string());
            }
            return Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand {
                command: "add".into(),
                uuid: arguments.get("folder").and_then(Value::as_str).map(str::to_string),
                name: None,
                url: Some(url.to_string()),
                title: arguments.get("title").and_then(Value::as_str).map(str::to_string),
                favicon_url: arguments.get("favicon_url").and_then(Value::as_str).map(str::to_string),
            }));
        }
```

- [ ] **Step 3: Build**

Run: `cargo build -p vmux_agent`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_agent/src/plugin.rs crates/vmux_mcp/src/tools.rs
git commit -m "feat(agent): apply bookmark commands via BookmarkOp"
```

---

## Phase 4 — Smoke test + verification

### Task 6: Tool-existence smoke test

**Files:**
- Modify: `crates/vmux_cli/tests/mcp_smoke.rs`

- [ ] **Step 1: Add the assertion**

Mirror `mcp_tools_list_includes_self_anchor_tools` (`mcp_smoke.rs:39`):

```rust
#[test]
fn mcp_tools_list_includes_bookmark_tools() {
    let stdin = "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/list\"}\n";
    let mut cmd = Command::cargo_bin("vmux").unwrap();
    cmd.arg("mcp")
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(contains("\"bookmark_list\""))
        .stdout(contains("\"bookmark_add\""))
        .stdout(contains("\"bookmark_folder_create\""));
}
```

- [ ] **Step 2: Run**

Run: `cargo test -p vmux_cli mcp_tools_list_includes_bookmark_tools`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_cli/tests/mcp_smoke.rs
git commit -m "test(cli): bookmark tools appear in MCP tools/list"
```

### Task 7: Workspace checks + manual MCP test

- [ ] **Step 1: Checks**

```bash
cargo fmt
git checkout -- patches/
cargo clippy -p vmux_service -p vmux_mcp -p vmux_agent -p vmux_cli --all-targets
cargo test -p vmux_service -p vmux_mcp -p vmux_cli
```
Expected: clean.

- [ ] **Step 2: Manual (user-run)**

With vmux running and the MCP server connected: call `bookmark_add {url}`, confirm it appears in the left chrome; `bookmark_list` returns it; `bookmark_folder_create`, `bookmark_pin {uuid}`, `bookmark_unpin {uuid}`, `bookmark_remove {uuid}` all reflect in the UI.

- [ ] **Step 3: PR** (`open-new-pr` skill).

---

## Self-Review

- **Coverage:** list ✓ (Task 4), add/remove ✓ (3,5), pin/unpin ✓ (3,5), folder_create ✓ (3,5). rename/remove-folder/move = future (UI plan also defers).
- **Type consistency:** `AgentCommand::BookmarkCommand` fields identical across protocol (T1), dispatch (T3/T5), handler (T5); `AgentQuery::BookmarkList` matches T1/T3/T4; `BookmarkOp` variants match the interactive plan's Task 2.
- **No new `AgentQueryResult` variant** — reuse `Spaces(String)`; zero edits to `vmux_mcp/protocol.rs` + `vmux_service/server.rs`.
- **Flagged adjustments:** exact `service`/`request_id`/`writers` field names + `ServiceAgentCommand` alias (copy from the `ListSpaces`/`SpaceCommand` arms); `folder`→`uuid` mapping for `bookmark_add` (T5 step 2).
