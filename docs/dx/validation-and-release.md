# Test and share a build

While editing, run the test for the behavior you changed. Before pushing, run the repository checks. Use a complete local bundle when checking packaging or native permissions.

## During an edit

These commands use layout as an example:

```sh
vmux internal cargo -- test --locked -p vmux_layout --lib
vmux internal cargo -- clippy --locked -p vmux_layout --all-targets
cargo fmt --all -- --check
```

Choose the crate and test that cover your change. Run builds and tests one at a time when they use the same workspace output. If you are about to build the app, use the build to catch compile errors instead of running a separate compile check first.

## Before a push

```sh
vmux dx test
```

This runs the development-command and CLI tests, formatting, ownership policy, and source conventions. The installed Git hook runs it on push too.

fmt checks formatting. Clippy checks Rust diagnostics. The repository's policy tests check Vmux-specific ownership and source rules. If a check fails, read its file/type diagnostic and fix the code or the actual exception; do not change a frozen fingerprint just to pass.

CI runs the affected tests and platform checks. A PR may reuse successful branch results only when its source and base match. Check CI on the latest commit before merging.

## Check a complete bundle

```sh
vmux build my-vmux -p local
vmux run my-vmux -p local
```

Use this when the change affects native integration, resources, or packaging. `vmux cache status` reports build storage, and `vmux cache clean` previews reclaimable caches; add `--apply` to clean them. Running apps and locked build outputs are retained. Try the behavior in that completed build. Use copied data for storage/migration changes.

## Choose how to share

A local build can be the finished result. Push to your fork if you want a durable personal branch or a reviewable PR. For an upstream contribution, keep the diff focused and explain the behavior and validation.

A distributable release has additional signing, resource, update-verification, and platform requirements. The release workflow defines the current steps and artifact names. Use your own product identity and update settings for an independently branded version; see [Make your own Vmux](customization.md).

## Preview these guides

Run the website for the branch you are editing:

```sh
vmux serve my-vmux
```

Open the URL shown in Terminal, then visit `/docs`. This starts the website development server and watches its styles while you edit the guides. For this documentation PR, the branch is `docs-guide`.

## Give a distributed build its own name

A personal code change can use the normal development build. For an independently named app, create a `.env` file at the root of your worktree:

```dotenv
VMUX_HOST_NAME=My Vmux
VMUX_HOST_NAMESPACE=com.yourname.myvmux
```

The first value is the app's display name. The second is its unique app identifier, which separates its data and service paths. Replace the example identifier with one for your product. Build and run with the same ordinary commands; Vmux reads this file from the selected worktree.

Distribution also needs your product's assets, permissions, signing, and update settings. Configure those before sharing an installer. They are separate from the first build-and-edit workflow.
