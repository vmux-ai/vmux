# Getting started

Build Vmux from source for your own version or an upstream contribution. Start from the public [vmux repository](https://github.com/vmux-ai/vmux), or clone your fork if you want to keep personal changes. The private relay repository is not a prerequisite.

## Prepare a checkout

macOS is the primary development and packaged host platform. iOS is the remote companion, and Linux is built and tested in CI. Windows and Android are not supported host targets yet.

Install Rust through rustup and the Apple command-line tools first. `vmux setup` then prepares the checkout's remaining native tools.

Create a feature worktree before editing:

```sh
git clone https://github.com/vmux-ai/vmux.git
cd vmux
git worktree add .worktrees/navigation -b navigation
cd .worktrees/navigation
```

Use the checkout's pinned toolchain and tool versions. Install the minimal development CLI without building the desktop or its browser runtime:

```sh
cargo install --path crates/app/vmux_cli --no-default-features --features dx
vmux setup
vmux doctor
vmux internal setup-hooks
```

`setup` prepares native tools; `doctor` reports missing or mismatched prerequisites. Rust and tool pins belong to the checkout, and the CEF manifest owns browser runtime provenance. The development CLI refreshes when its compiler inputs change.

## Build and inspect a change

```sh
vmux build -b navigation
vmux run -b navigation
```

`build` compiles and signs without launching the app. `run` opens a completed artifact and does not compile a missing one. Branch names resolve to existing worktrees; ambiguous aliases are refused.

Use the development profile for iteration. Use a local package for packaging, permissions, or distribution-like behavior:

```sh
vmux build -b navigation -p local
vmux run -b navigation -p local
```

`-P/--platform` chooses a target such as `ios`; `-p/--profile` chooses the build profile. Command help defines the current options.

Agents prepare builds and inspect logs; the person performs app QA. Agents do not launch or automate Vmux.

## Protect existing work

Builds enforce target locks and storage policy. Run one build or Cargo test invocation at a time per workspace. `vmux cache status` inspects storage; `vmux cache clean` previews cleanup and `--apply` performs it under the lock.

Development and local builds can share persistent profile data. Use `VMUX_DATA_DIR` with an absolute path to a copy for persistence, schema, or migration tests. Never use `VMUX_TEST=1` with real shared data. Respect GUI/service data leases when changing builds or profiles.

## Make your first change

For a personal variant, start with [Make your own Vmux](customization.md). Read [Architecture](architecture.md) and [Feature development](feature-development.md) to locate the owner of a behavior change. Start at the owning feature's root plugin and follow its typed requests, projections, and behavioral tests. [AGENTS.md](https://github.com/vmux-ai/vmux/blob/main/AGENTS.md) holds repository workflow rules; [Validation and release](validation-and-release.md) explains the checks that enforce them.

## Develop Vmux from inside Vmux

Open the public Vmux repository as a Project, create the feature worktree, and align a Space with that checkout. Give a Session a focused outcome, such as investigating a bug or preparing a tested change. Keep Code, Git, Browser, and Terminal beside the conversation to inspect its inputs and result.

The same CLI commands prepare the branch's app while the installed Vmux hosts the development Session. Agents build and report artifacts; the person launches the completed branch for app QA. Use copied data when the work concerns storage or schemas.

A first change can follow this loop: locate the owning plugin, trace its request and state, make one focused change, run its targeted test and the pre-push gate, then inspect the diff. Keep the result locally, open a PR in your fork, or propose a general improvement upstream. Share the goal, validation, and required QA so another Session can review it without rediscovering the task.

An automated in-product self-build-to-PR flow is future work. This reproducible checkout, validation, and review path is the entry point that workflow can reuse.
