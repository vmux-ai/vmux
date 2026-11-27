# Getting started

Build Vmux, make one visible change, and try it. You can keep the result as your own version or contribute it upstream.

## Get a checkout running

You need a Mac, [Rust](https://rustup.rs/), and Apple's command-line tools.

```sh
git clone https://github.com/vmux-ai/vmux.git
cd vmux
git worktree add .worktrees/my-vmux -b my-vmux
cd .worktrees/my-vmux
```

A worktree is another directory for a branch. This one holds your changes and builds. Use your fork's clone URL if you want to save personal changes there.

Install the Vmux development command, prepare its tools, and check the setup:

```sh
cargo install --path crates/app/vmux_cli --no-default-features --features dx
vmux setup
vmux doctor
vmux internal setup-hooks
```

Build your branch and open it:

```sh
vmux build my-vmux
vmux desktop my-vmux
```

`build` prepares the app; `desktop` (or `run`) opens the finished build without compiling, or builds it first if no completed artifact exists. After an edit, rebuild with `vmux build my-vmux` and open again. If the build reports an error, fix it before running the app.

## Make your first change

Open `crates/feature/vmux_terminal/assets/index.css`. Add `font-weight: 700;` inside its existing `body` rule, rebuild, and open Terminal. Ordinary terminal text should now appear bold. Remove the line to return to the original appearance.

This first edit uses ordinary CSS. The UI components themselves use Dioxus, whose `rsx!` syntax plays the same role as JSX in React. [Make a change](feature-development.md) follows a screen from its styles into callbacks and application behavior.

The installed Git hook checks formatting when you push. CI runs the development tooling,
CLI, Clippy, tests, and repository policies. For explicit local diagnostics, run:

```sh
vmux dx test
```

This checks the development tooling, CLI, formatting, and repository policies. Use it when
investigating a failure; it is not required before every push.

## Where to go next

[How Vmux works](architecture.md) explains where shared state lives and how a UI request reaches its handler. [Keep your own version](customization.md) covers a personal fork and sharing. You can read those as you need them.

You can use Vmux itself as the editor: open this checkout as a Project, select the worktree in its Space, and keep Code, Terminal, and a Session together. The same build and test commands work whether you edit manually or ask an agent to make the change.

`vmux desktop <branch>` opens the desktop app for that branch without compiling when a
completed artifact matching the current commit already exists, and builds it first when
missing or outdated. Pass `--no-build` to fail instead of compiling. `vmux desktop` uses
the current checkout; select `vmux desktop main` when you intend main. Use
`vmux build <branch>` to build without launching. `vmux run` remains available for
explicit platform selection, including mobile and web.

## Interactive TUI and themes

In an interactive terminal, developer commands show live step progress, elapsed time,
summaries, and recovery hints, and `vmux cache clean` offers an inline confirmation when
reclaimable caches are found. Run `vmux tui` to open the interactive multi-pane developer
cockpit for picking a worktree, build profile, command, and color theme:

```sh
vmux tui
vmux tui --list-themes
vmux tui --theme tokyo-night
```

Themes match the desktop app's built-in color schemes and persist between invocations
(`VMUX_TUI_THEME` overrides per shell). Non-TTY sessions, `CI`, and `VMUX_NO_TUI=1`
automatically fall back to concise single-line plain-text output without prompting.

## CLI updates

The development CLI refreshes from the selected checkout when its source changes.
Standalone releases currently cover macOS Apple Silicon and contain the DX-only CLI.
They use the desktop's `stable` (default) and `preview` release channels:

```sh
vmux update --channel preview
vmux update --channel stable
vmux update --check
vmux update --auto off
```

Channel and automatic-update preferences persist between commands. Signed standalone
CLIs check at most hourly on invocation and keep working with the installed version
if an automatic update fails.

Cargo source installs without `VMUX_UPDATE_PUBLIC_KEY` refresh from source and can
inspect release metadata, but need a verification key to install releases. Standalone
releases embed the desktop update key. Runtime-enabled and app-bundled CLIs keep their
existing source/application update owner.

## Native VM dependencies

Container development requires Apple Silicon, macOS 14+, libkrun 1.x, libkrunfw 5,
and gvproxy. Declare the dependencies in your Homebrew Brewfile, then run
`brew bundle`:

```ruby
tap "slp/krun"
brew "slp/krun/libkrun"
brew "slp/krun/gvproxy"
```

Build and open your branch with the commands above. [VM and Docker](../ux/containers.md)
covers starting the VM, discovering containers, and forwarding their ports.
