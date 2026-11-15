# Keep your own version

Your own Vmux can start with a change to one screen, shortcut, or feature. Follow [Getting started](getting-started.md), then build and try your change. A local build can be the finished result.

## Save it in your fork

Create a fork on GitHub and clone that repository when starting your checkout. Keep each change on its own branch and worktree. Push the branch to your fork when you want to save it or open a PR for review.

An upstream PR is optional. Share a change there when it would be useful to other Vmux users; explain the behavior and how you tested it. Personal preferences can stay in your own version.

## Change a feature

Start with the screen or interaction you want to improve. [Make a change](feature-development.md) shows how to find its UI and handler, try an edit, and run the relevant checks.

For a different feature set, look at the plugins selected by `vmux_app`. [How Vmux works](architecture.md) explains how those pieces fit together. Reusing the pieces in an entirely separate application is covered by the optional [Framework guide](framework.md).

## Share an installable app

When you want to distribute a separately named product, it also needs its own app identifier and signing/update settings. [Test and share a build](validation-and-release.md) covers that step. These settings are optional for your first code change.
