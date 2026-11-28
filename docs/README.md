# Vmux docs

Use Vmux as a browser, then bring an assistant, notes, files, and commands into the task you want to finish. You can also build and change Vmux itself.

## UX: use Vmux

Start with [Start with Vmux](ux/getting-things-done.md), written for someone coming from Chrome.

### Follow a task

- [Find and book a flight](ux/book-a-flight.md).
- [Buy something](ux/shopping.md).
- [Cook dinner](ux/cooking.md).
- [Fix a bug and make a PR](ux/make-a-pr.md).

### Learn a feature when you need it

- [Layout](ux/layout.md): arrange tabs and panes, choose focus, and return to work.
- [Projects and Sessions](ux/workspace.md): organize a goal and related conversations.
- [Browser](ux/browser.md): open sites and compare sources.
- [Bookmarks](ux/bookmarks.md): keep quick access and grouped references.
- [Knowledge](ux/knowledge.md): save and reuse notes, preferences, and decisions.
- [Agents](ux/agents.md): give a task, follow tool activity, and review the result.
- [Code and Git](ux/code-and-git.md): edit files, inspect a diff, and share changes.
- [Terminal](ux/terminal.md): run commands and return to processes.
- [VM and Docker](ux/containers.md): manage the Linux VM, containers and Compose groups.
- [Remote access](ux/remote.md): continue supported host work from the phone.
- [Feature flags](ux/features.md): inspect values and review proposed publications.

## DX: build and change Vmux

1. [Getting started](dx/getting-started.md): run a checkout and make one visible change.
2. [How Vmux works](dx/architecture.md): connect familiar React ideas to Dioxus and Bevy.
3. [Make a change](dx/feature-development.md): follow a screen into its callback and handler.
4. [Test and share a build](dx/validation-and-release.md): check the result and share it.

[Keep your own version](dx/customization.md) covers a personal fork.

## Vmux Framework: build another app

- [Framework overview](dx/framework.md): examples, SDK validation, and version pinning.
- [Framework quickstart](dx/framework/framework-quickstart.md): run a CEF-free desktop window or headless ECS app in minutes.
- [Core concepts](dx/framework/framework-core-concepts.md): the eight primitives, entity vocabulary, operations, async, and autosave.
- [Windows, pages, and menus](dx/framework/framework-windows-and-pages.md): `WindowHostPlugin`, `PageHostPlugin`, `UiState`/`UiEvent`, menus, and web hosting.
- [Testing and plugin authoring](dx/framework/framework-testing-and-plugins.md): deterministic headless testing, `feature.ron` composition, and product namespaces.
- [Crates and macros reference](dx/framework/framework-reference.md): `vmux_app` feature matrix, macro cheat sheet, and target support.
