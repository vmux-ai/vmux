# Vmux docs

Vmux combines a browser, an IDE, terminals, and ACP agents in one workspace. These guides explain the experience to preserve and the architecture to extend. Code, tests, typed contracts, and feature manifests define exact behavior.

## UX

- [Get something done](ux/getting-things-done.md): turn a goal into Project Sessions, independent tasks, and a reviewed result.
- [Projects, Spaces, and Sessions](ux/workspace.md): organize related work and keep the right checkout and surfaces in view.
- [Browser](ux/browser.md): research, reproduce behavior, and inspect the running product.
- [Code and Git](ux/code-and-git.md): inspect a checkout, make a focused change, and review its diff.
- [Terminal](ux/terminal.md): run project commands and return to ongoing processes.
- [Agents](ux/agents.md): delegate tasks, inspect activity, and review permission requests and outcomes.
- [Remote access](ux/remote.md): pair the phone and reconnect to host work.

## DX

- [Getting started](dx/getting-started.md): prepare a checkout, build a branch, and inspect the result.
- [Make your own Vmux](dx/customization.md): personal builds, feature composition, branding, fork PRs, and optional upstream contributions.
- [Architecture](dx/architecture.md): plugin composition, ECS ownership, runtime boundaries, and crate layout.
- [Feature development](dx/feature-development.md): find the owner and trace intent through state, IO, and presentation.
- [Validation and release](dx/validation-and-release.md): targeted checks, automated policy, CI, and distribution.
- [Vmux Framework](dx/framework.md): reuse capabilities in an independent application.

Start with Get something done to use Vmux. For a first build or personal version, read Getting started and Make your own Vmux; use Architecture and Feature development when changing behavior. Then inspect the owning feature's plugin, manifest, and behavioral tests.

## Keeping the guides useful

Document durable decisions and user expectations in the relevant topic, in the PR that changes them. Keep implementation details beside their types and tests. Do not copy module inventories, current field lists, or debugging history into the guides. Repository workflow rules live in [AGENTS.md](https://github.com/vmux-ai/vmux/blob/main/AGENTS.md); executable checks enforce coding policy.
