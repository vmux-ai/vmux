# Agent Instructions

## Communication Style

Use caveman mode. Terse, direct, no filler. Execute first, talk second. No meta-commentary, no preamble, no postamble. Code speaks.

## Pre-commit Checks

CI runs fmt, clippy, and tests for PRs.

Run targeted tests during the edit loop when they clarify non-trivial behavior, reproduce a regression, or verify a risky change. TDD is useful but not mandatory; do not add tests mechanically for every edit. Run workspace-wide local checks manually only when the user asks; the pre-push hook remains the required final fmt, clippy, and test gate.

**Install that hook in every new worktree — `./scripts/setup-hooks.sh`.** Hooks live under `.git/`, which a fresh worktree does not inherit, so the gate this file leans on is simply absent until you run it. Nothing announces that; the first sign is a push that should have been stopped and wasn't. The reverse failure is worse and quieter: believing there is no gate, you hand-run `clippy --workspace` and `test --workspace` on every change, which costs half an hour each and — because cargo takes one lock per target directory — starves anything else building beside you.

Never run two cargo invocations against the same target directory at once. The second does not fail, it queues, so both take far longer than either alone and neither reports why; worse, tests that spawn subprocesses start failing on timeouts that have nothing to do with the code. A workspace test racing a release build is enough to do it.

If a change affects an excluded patched CEF crate, run the appropriate package checks too.

If any check fails, fix the issue before committing. Do not push broken code.

## Test Quality

Keep the suite lean. Every test must have an independent oracle and catch a plausible behavioral regression or explicit repository-policy violation.

Do not add tests that only:

- Assert static Tailwind/CSS class literals, source-code substrings, or implementation shape.
- Read Rust source with `include_str!`/`read_to_string` and check `contains(...)`.
- Restate a constant, constructor input, struct field, enum payload, derived `Default`, or framework behavior.
- Check one item in a registry/list when one table-driven exact-set test can cover the contract.
- Duplicate behavior already covered by a stronger integration or boundary test.
- Silently pass when a required fixture or generated artifact is missing.

Prefer tests for observable behavior: pure decision logic, parser and serialization contracts, persistence, typed ECS message/system integration, and packaging/runtime invariants. For important UI behavior, test rendered DOM, computed state, interaction output, or a visual regression; otherwise leave styling untested.

Source scans are allowed only for explicit repository-wide policies or packaging invariants that cannot reasonably be tested through behavior. For these tests, the protected invariant is the independent oracle. Keep them narrow and state that invariant in the test name.

Before adding a test, verify that it would fail under a realistic bug and remain valid after a behavior-preserving refactor. If not, do not add it.

## Debugging

**Never launch or run vmux yourself.** Do not execute `make dev`, `vmux_desktop`, `Vmux.app`, or automate input against the app. Build it when needed, then ask the user to run the normal build. After the user runs it, inspect the app logs directly.

When adding temporary diagnostics to investigate a bug, make logging unconditional (default-on) — never gate it behind an env var or flag the user must set. The user runs the normal build; logs must appear without extra setup. Strip every temporary diagnostic before committing the fix.

**Always read the app's own logs in Application Support first** — do not ask the user to capture stderr. After the user runs the app, read these files yourself directly; never ask them to paste, relay, or summarize log contents. vmux writes them to:

- `~/Library/Application Support/Vmux/<build-profile>/logs/vmux-<build-profile>.<date>.log` — the Bevy/tracing output (`info!`/`warn!`/`error!`). For the `dev` build that's `…/Vmux/dev/logs/vmux-dev.<date>.log`.
- `~/Library/Application Support/Vmux/<build-profile>/profiles/<profile>/chrome_debug.log` — CEF/Chromium plus the page JS console (surfaced via `display_handler`).

Diagnostics must use the tracing macros (`bevy::log::info!`/`warn!`) to land in the tracing log — **raw `eprintln!`/stderr is NOT captured there**.

## Platform-Specific Code

This project targets macOS (primary) and Linux (CI). When adding imports or code that uses platform-specific APIs (CEF, winit, AppKit), always add appropriate `#[cfg(...)]` gates. Let rustfmt reorder cfg-gated imports.

## Code Style

Prefer plain, imperative control flow. Early returns, `match`, `if let` / `let ... else`, and `for` loops read better under pressure than combinator chains, and they produce better error messages and stack traces.

Avoid functional-style composition as a default: long `map` / `and_then` / `filter_map` / `fold` chains, closures passed in to build values, and iterator pipelines that assemble state. Reach for a builder or an explicit loop instead.

Combinators are fine where they stay small and local — `map_err` to convert an error, `unwrap_or` for a default, a `map` that drops one half of a tuple. The line is whether a reader has to hold intermediate state in their head to follow it.

`crates/vmux_service/src/remote/quic/dispatch.rs` is the reference: one `match` on the request, early returns for every refusal, and a single choke point that every session mutation passes through.

## Rules

- Route every new user-facing UI string through Fluent (`vmux_ui::i18n`). Add the message to `en-US.ftl` and every bundled locale with identical IDs and variables. Do not ship untranslated English literals or rely on English fallback for bundled locales. Dynamic external content and raw diagnostic output are exempt.
- No comments in Rust sources. Not `//`, not `///`, not `//!`. Names and types carry what the code does; `docs/architecture.md` carries why the system is shaped the way it is. A *why* worth writing down goes there, where a reader finds it without opening a file and where one paragraph serves the ten call sites that would each have grown their own. This does not apply to `patches/`, which is vendored third-party code we re-apply on every version bump.
- Leave one blank line between an item with a body and whatever follows it. A `fn`, `impl`, `struct` or `mod` that ends in `}` must be followed by a blank line before the next declaration or its attributes — jamming them together turns a file into one wall of text, and it is the usual casualty of splitting a large module. Declarations without bodies stay packed: consecutive `use`, `mod foo;` or `const` lines want no gaps, and neither does an attribute sitting on the declaration it belongs to. Inside a function body it is your judgement — two guard clauses in a row read better touching. rustfmt cannot express this (`blank_lines_lower_bound` is unstable and separates *every* statement), so `every_item_is_separated_from_the_body_above_it` in `vmux_desktop` enforces it.
- **A crate lives in `crates/app/`, `crates/page/`, `crates/host/`, or flat in `crates/`.** `app/` is something a user starts — the desktop app, the iOS app, the CLI. `page/` is a crate that answers a URL (`vmux://terminal`, `file://`), one per page; if it does not serve a URL it does not go there. `host/` is runtime that runs with no UI and owns state — the service, the relay client, the MCP crates. Everything else stays flat: shared libraries, and `vmux_browser`, which composes pages into the desktop shell and therefore sits above `page/` and below `app/` — a `page/` crate must never depend on it, and nothing but `app/vmux_desktop` may. Two traps. `host/` is **not** a layer above `page/`: `page/vmux_agent` depends on `host/vmux_service`, `host/vmux_mcp` and `host/vmux_remote`, because those crates cfg-split and a page links only the non-host half. And the cfg alias `host` is **not** the directory: `vmux_ui` and `page/vmux_layout` hold host-gated code while staying out of `host/`, and `vmux_git` and `vmux_profile` stay flat because page crates and `vmux_core` depend on them. The directory means "this crate's job is host-side work", not "this crate contains host-gated code".
- Never add or commit `.claude/*` files. They are local agent config, not project files.
- Do not use mod.rs files. Use the filename-based module pattern (e.g. `layout.rs` + `layout/` directory).
- Do not create a module directory holding a single file. `foo.rs` plus `foo/bar.rs` and nothing else is one module wearing two filenames — make `bar` a sibling of `foo` instead. A directory earns its place at two children. Nesting also lets the child reach the parent's *private* imports through `use super::*`, so a split that looks like separation quietly isn't; siblings have to name what they use.
- Behaviour hangs off a type, not loose in a module. A function that takes a value and returns something derived from it belongs to that value's type — `ToolActivity::of(name)`, not `tool_activity(name)`; `entry.reference()`, not `media_reference(entry)`. This is what keeps a module from becoming a bag of verbs that any caller may combine in any order, and it is how the type stays the place you look to find out what it can do. The exceptions are narrow: entry points a framework calls by shape (`main`, `#[test]` functions, Bevy systems, Dioxus components), and a private helper used once by the item directly above it. A helper with no obvious owning type usually means the type is missing, not that the rule does not apply — when in doubt introduce the type, even if its only field is the thing being passed around. This holds inside `mod tests` too: a test fixture is behaviour like any other, so it hangs off the type it builds (`Relay::start()`), and a fixture that only wraps a constructor should be the constructor.
- Declare a file's entry point at the top, above what it composes, and order the rest by composition — a component's children follow it, not the other way round. For a Bevy module that is its `Plugin`; for a Dioxus page it is the root component (`Page`, `App`). The entry point is the table of contents — what the file spawns, schedules or renders — and reading it first is what makes the rest navigable. Burying it behind helpers hides that.
- A plugin with platform-specific code splits into platform plugins rather than growing `#[cfg]` inside one. `FooPlugin` composes `add_plugins((FooNativePlugin, FooMobilePlugin))`, each gated once at its own module, so what runs where is visible from the composition instead of having to be reconstructed from attributes scattered through a build method.
- Split a large plugin by feature, not by item kind. `approval.rs` holding its components, its systems, its messages and its tests beats `components.rs` + `systems.rs` + `events.rs`, because a change is almost always to one behaviour and the by-kind split spreads that behaviour across every file while putting unrelated things next to each other. This is what Bevy itself does — `RenderPlugin` composes `CameraPlugin`, `MeshPlugin`, `ViewPlugin` — and it makes the parent plugin a table of contents. A slice earns a directory at two children, the same as any other module; do not pre-split one feature into by-kind files.
- When configuring a Bevy `App` in plugins or tests, chain consecutive `App` builder calls in one expression (e.g. `app.add_plugins(...).init_resource::<T>().add_systems(...);`) instead of separate `app.*;` statements. Do not chain `app.world()`, `app.world_mut()`, `app.update()`, or control-flow-dependent mutations.
- Prefer Bevy system + message integration over direct helper-function calls for cross-module behavior. Register message types and systems in plugins/tests, send typed messages, run schedules, and assert on resulting ECS state/messages instead of bypassing production flow with ad hoc helpers.
- **A system is private to the module whose plugin registers it, and a test must not widen that.** If a system is `pub`/`pub(crate)`, the plugin that schedules it is not the only way in, so its ordering, its set and its run conditions are no longer guaranteed by anything. The usual culprit is a test reaching past the plugin to `add_systems` the function directly, which forces the visibility and then quietly tests a different schedule from the one that ships — a system registered without its `.in_set(..)` runs in a different order than production, so the test can pass on an ordering the app never has. Add the plugin in the test instead and leave the system `fn`.
- **A module does not export a bag of verbs for another module to compose.** A long `use super::thing::{a, b, c, ...}` of free functions is the symptom: nothing then says which order they may be called in, or which are meaningful together, so the knowledge lives in the caller and is duplicated by the next one. Hang them off the type they operate on, or make the module a nested `Plugin` that owns its own systems and exposes messages instead of functions. The parent plugin composing children is the table of contents; a pile of imported verbs is the opposite.
- **Anything that returns `Element` is a component: `#[component]`, PascalCase, rendered as `Foo { .. }`.** Never write a plain `fn thing(..) -> Element` and call it as `{thing(..)}`. A helper is inlined into its caller's scope, so it re-runs whenever the caller does and cannot memoize on its inputs; a component owns a scope and skips re-render when its props are unchanged. Take owned props — `String`, `Vec<T>` — rather than borrowing to keep a helper signature. `every_component_is_named_like_an_element` in `vmux_ui` enforces the naming; Rust's own lints cannot, because every page carries `#![allow(non_snake_case)]` to permit the convention at all.
- **Never use `bevy::winit::UpdateMode::Continuous`.** It causes 100-200% idle CPU. Use `UpdateMode::Reactive` or `UpdateMode::reactive_low_power`. If input/scroll/animation lags, the fix is to route the missing wake source through `EventLoopProxy::send_event(WinitUserEvent::WakeUp)` — not to switch to Continuous. The CEF wake throttler (`MessageLoopWakePolicy` + `cef-wake-throttle` thread) already wakes the loop at display refresh rate when CEF schedules pump work. A `no_continuous_update_mode` test in `vmux_desktop` enforces this.

## Linear

When taking a Linear issue (e.g. "take VMX-XX"), immediately move it to **In Progress** before doing anything else — before creating a worktree, before reading code, before drafting a PR.

## Worktrees

**Never edit files on the main worktree.** All changes must happen inside a feature worktree. Before writing any code for a Linear issue:

1. Check if a worktree already exists: `git worktree list`
2. Create worktree if needed: `git worktree add .worktrees/vmx-<number> -b <branch-name>` — always name the worktree directory using the `vmx-<number>` convention matching the Linear issue (e.g., `.worktrees/vmx-88`).
3. `cd` into the worktree directory and make all edits there. The first build through `make` automatically seeds its build cache from the main worktree.
4. When done, commit on the feature branch, push it, and open a PR. Never merge feature work into local `main`.
5. After the PR is merged remotely, remove the worktree: `git worktree remove .worktrees/<short-name>`
6. Remember: if the worktree is deleted while your shell is inside it, `cd` back to the repo root — `../..` won't work.

Worktree directory: `.worktrees/` (already in `.gitignore`).

## Merging

Before merging any PR:

1. **Check review comments.** Read all review feedback — CodeRabbit and human reviewers — and address or explicitly triage every item. A green status check is not enough; unresolved review comments must be handled first. **Reply to every CodeRabbit thread** — either reflect the fix in code (cite the commit) or comment a triage reason — so no thread is left dangling, then resolve them (e.g. `@coderabbitai resolve`).
2. **Check CI.** Confirm all required checks are green on the PR's head commit.

After merging, clean up: remove the worktree (`git worktree remove .worktrees/<name>`) and delete the branch (`gh pr merge --delete-branch` for the remote; delete the local branch too if it lingers).

## Documentation

- **No design specs, no plan files, no dated design records.** Git, GitHub and Linear already hold the history, and a copy in the tree is one that goes stale silently — with nothing to catch it, because no test reads a spec. Write the decision into the architecture doc it changes, in the same PR as the behaviour.
- **`docs/architecture.md` is the only file in `docs/`, and the only design document.** vmux.ai serves it as the single `/docs` page, via `include_str!` in `website/src/docs.rs` — so a change to it ships with the site. It is not an index and it does not link out to deep dives; a subject that will not fit belongs in the prose of the section that covers it, or does not belong yet. Keep it short and keep the diagrams.
- There is no API reference and no `vmux_docs` generator. Rustdoc therefore has no consumer, which is why the no-comments rule above admits no exception for it.
- **Relay internals belong to `vmux-cloud`, not here.** This repo says what the client sends, what it pins, and what it does when the relay is unreachable. It does not explain the registry, tag routing, admission limits or deployment. The *Three roles* section names the relay as a node and then defers, which is the boundary to hold.

## Git

Never merge, rebase, or cherry-pick feature work into local `main`. Land changes through PRs only.

Always prefer `git rebase` over `git merge` when updating branches. Use `git push --force-with-lease` after rebasing.

When asked to create or open a PR, create it directly with `gh pr create`. Do not use `-w`, stop at a prefilled browser form, or require the user to submit it. Return the created PR URL.
