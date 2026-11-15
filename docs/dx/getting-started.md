# Getting started

This guide takes you from a source checkout to a running Vmux build. You can use it to make your own version or prepare a contribution.

## Before you start

Use a Mac with [Rust installed through rustup](https://rustup.rs/) and Apple's command-line tools. macOS is the main development platform. Linux runs the test suite in CI; the iPhone app connects to a host remotely.

## Get the source

Clone Vmux, or use your fork's URL if you want to keep personal changes:

```sh
git clone https://github.com/vmux-ai/vmux.git
cd vmux
git worktree add .worktrees/my-vmux -b my-vmux
cd .worktrees/my-vmux
```

A worktree gives this branch its own directory. Keep your edits and builds there.

## Install the development commands

```sh
cargo install --path crates/app/vmux_cli --no-default-features --features dx
vmux setup
vmux doctor
vmux internal setup-hooks
```

`setup` installs the tools the checkout needs. `doctor` tells you what is missing. The last command installs the checks that run before a push.

## Build and open Vmux

```sh
vmux build -b my-vmux
vmux run -b my-vmux
```

The first command builds and signs the app. The second opens the completed build. If the build failed, fix that error before trying to run it.

Use this development build while changing code. When you need to check a complete local bundle, run:

```sh
vmux build -b my-vmux -p local
vmux run -b my-vmux -p local
```

## Make a small change

Read [How Vmux works](architecture.md) next. It explains the UI and state model using familiar React concepts. [Make a change](feature-development.md) shows where to look in the code and how to test a feature.

Keep your first change small: reproduce a problem, find its handler, change the behavior, and run the relevant test. You can keep the build locally, push it to your fork, or open an upstream PR. [Make your own Vmux](customization.md) covers a separate name and product settings.

If you use the installed Vmux as your editor, open this repository as a Project and select the worktree for your Space. Browser, Code, Terminal, and Sessions can stay beside each other while you work. The build commands are the same whether you type them yourself or ask an agent to prepare a change.

When testing storage or migrations, point `VMUX_DATA_DIR` at an absolute path to a copy of the data. Normal builds can share profile data; using a copy keeps your everyday workspace safe.
