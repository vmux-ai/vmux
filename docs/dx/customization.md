# Make your own Vmux

You can build a personal version of Vmux without making an upstream contribution. Use the same source, feature architecture, and developer commands, then decide whether the result should remain local, live in your fork, or be shared upstream.

## Start from a working build

Follow [Getting started](getting-started.md) and verify a normal build before customizing it. Use a feature worktree for the change and keep its Project/Space/Session context clear. A small first variation is easier to inspect, validate, and maintain than several unrelated changes at once.

If you want a durable personal branch, fork the public repository and clone your fork. Keep the upstream repository as a remote so later updates can be reviewed and incorporated through the normal Git workflow.

## Choose what to change

For a behavior change, find the owning feature and extend its plugin, typed intent, and state. For a different composition, select the capabilities the host should compile/install. Existing runtime roots compose features; their public contracts and manifests show the supported boundaries.

Use [Feature development](feature-development.md) for product changes and [Vmux Framework](framework.md) when building a distinct application from reusable capabilities. Exact switches and APIs remain defined by the selected revision's code and help.

## Give a branded version its own identity

The product build supports a distinct name and reverse-domain namespace together:

```sh
VMUX_HOST_NAME="Orbit" VMUX_HOST_NAMESPACE="org.example.orbit" vmux build -b orbit -p local
VMUX_HOST_NAME="Orbit" VMUX_HOST_NAMESPACE="org.example.orbit" vmux run -b orbit -p local
```

Use your actual worktree branch in place of `orbit`. Keep the accepted product inputs consistent between build, launch, and packaging. Product identity separates data, configuration, service, and credential namespaces; a matching display name is not permission to use another product's data.

Custom products default to their own update configuration and limited native media capabilities. Configure supported assets, permissions, and update endpoints through the product build metadata when your host needs them. Do not assume an independently branded version can use the official Vmux update channel or signing identity.

## Validate and choose how to share

Run the relevant behavior checks and pre-push gate, inspect the diff, and try the prepared artifact with copied data when storage/schema changes are involved. Keep the personal configuration and product boundary explicit.

A local build is a valid outcome. A PR in your fork provides a review point for your own version. An upstream PR is optional and should describe the general behavior it improves, its validation, and any compatibility implications rather than require everyone to adopt personal settings.

## Work on it from inside Vmux

The installed Vmux can host the Project and Sessions while the CLI builds the customized checkout. Agents can prepare changes and artifacts for review; you launch and evaluate the result. A future in-product self-build-to-PR workflow can reuse this same checked-out source, scoped task, validation, and sharing path.
