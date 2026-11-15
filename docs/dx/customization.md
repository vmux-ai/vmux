# Make your own Vmux

You can keep a personal Vmux build without contributing it upstream. Start with a working source build from [Getting started](getting-started.md), then make one change at a time.

## Keep personal changes in a fork

Fork the repository if you want to save and share your version. Clone that fork and use a worktree for each change. A PR in your own fork is useful for reviewing your work; an upstream PR is optional.

For a behavior change, follow [Make a change](feature-development.md). If you want a different set of features, look at how `vmux_app` connects plugins and what the chosen host enables. The code and command help show the available options for your revision.

## Change the name

A separately branded build needs both a name and a namespace. For example, with an existing `orbit` worktree:

```sh
VMUX_HOST_NAME="Orbit" VMUX_HOST_NAMESPACE="org.example.orbit" vmux build -b orbit -p local
VMUX_HOST_NAME="Orbit" VMUX_HOST_NAMESPACE="org.example.orbit" vmux run -b orbit -p local
```

Use your branch name and your own reverse-domain namespace. Use the same settings for building and running. The namespace separates the app's data, configuration, service, and credentials from Vmux.

A custom product does not automatically use Vmux's official update or signing configuration. Its native permissions and assets also need the settings for that product. The product build code defines those options; configure them before distributing the build.

## Test and share the result

Try the changed behavior, run its tests, and run `vmux internal pre-push`. Use copied data when changing storage or migrations.

You can stop at a local build, push the branch to your fork, or open an upstream PR for a generally useful change. Explain what changed and how you checked it.

You can do this work inside the installed Vmux: open the fork as a Project, use its worktree in a Space, and keep the conversation, Code, Git, and Terminal together. The source/build workflow is the same whether you edit manually or use an agent.
