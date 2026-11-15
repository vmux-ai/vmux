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
vmux run my-vmux
```

`build` prepares the app; `run` opens the finished build. After an edit, repeat those two commands. If the build reports an error, fix it before running the app.

## Make your first change

Open `crates/feature/vmux_terminal/assets/index.css`. Add `font-weight: 700;` inside its existing `body` rule, rebuild, and open Terminal. Ordinary terminal text should now appear bold. Remove the line to return to the original appearance.

This first edit uses ordinary CSS. The UI components themselves use Dioxus, whose `rsx!` syntax plays the same role as JSX in React. [Make a change](feature-development.md) follows a screen from its styles into callbacks and application behavior.

Before pushing, run:

```sh
vmux dx test
```

This checks the development tooling, CLI, formatting, and repository policies. The Git hook repeats these checks when you push.

## Where to go next

[How Vmux works](architecture.md) explains where shared state lives and how a UI request reaches its handler. [Keep your own version](customization.md) covers a personal fork and sharing. You can read those as you need them.

You can use Vmux itself as the editor: open this checkout as a Project, select the worktree in its Space, and keep Code, Terminal, and a Session together. The same build and test commands work whether you edit manually or ask an agent to make the change.
