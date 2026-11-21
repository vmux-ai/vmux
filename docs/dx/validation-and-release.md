# Test and share a build

While editing, run the test for the behavior you changed. The push hook checks formatting;
CI runs the heavier repository checks. Use a complete local bundle when checking packaging
or native permissions.

## During an edit

These commands use layout as an example:

```sh
vmux internal cargo -- test --locked -p vmux_layout --lib
vmux internal cargo -- clippy --locked -p vmux_layout --all-targets
cargo fmt --all -- --check
```

Choose the crate and test that cover your change. Run builds and tests one at a time when they use the same workspace output. If you are about to build the app, use the build to catch compile errors instead of running a separate compile check first.

## Explicit local diagnostics

```sh
vmux dx test
```

This runs the development-command and CLI tests, formatting, ownership policy, and source
conventions. Use it to investigate a failure or validate a tooling change. The installed
Git hook runs only `cargo fmt --all -- --check`, without compiling or requiring a prepared
development CLI. Reinstalling the hook is unnecessary for existing checkouts because its
transport reads the script from the worktree being pushed.

fmt checks formatting. Clippy checks Rust diagnostics. The repository's policy tests check Vmux-specific ownership and source rules. If a check fails, read its file/type diagnostic and fix the code or the actual exception; do not change a frozen fingerprint just to pass.

CI validates each PR's captured merge with its target branch once. Feature pushes do not start another CI run. Formatting, Clippy, dependency boundaries, repository policy, and affected Rust tests remain required; compatibility checks run for SDK/framework inputs or affected lean desktop features. Immutable Git dependency checks run on main and scheduled or manual validation.

A job can reuse an earlier successful check in the same PR when its target base, execution scope, tracked inputs and modes, CI definition, and tool settings match. Input claims are published before validation and accepted only with the matching completed successful job attempt. Missing evidence or a newer failed, cancelled, or incomplete job requires fresh validation; lookup errors also fall back to running checks. Fork, release, main, and manual runs do not reuse PR results. Rust fingerprints conservatively include workspace and dependency sources; this skips unaffected jobs, not individual crates or tests.

The full ownership audit runs under Lint. Ordinary Rust tests exclude that same expensive audit while retaining the other tests; the required quality gate includes the audit's result.

The CI names describe the validation target:

| Job or step | What it validates |
|---|---|
| Build CI tool & select checks | Reuses a verified development CLI when its compile inputs match, otherwise builds it, then selects affected checks. |
| Rust unit & integration tests | Runs the selected Rust packages' unit, integration, and documentation tests. |
| Public API compatibility tests | Uses the public framework and host APIs from independent workspaces. |
| Desktop feature availability tests | Checks screenshot, recording, and update responses when those features are unavailable. |
| App configuration tests | Checks plugin selection, dependencies, and core lifecycles without desktop adapters. |
| ECS Policy check, inside Lint | Checks ECS ownership rules and repository source conventions. |

`Check Results` is the required GitHub gate for `main` and the `onboarding` integration branch. It waits for every selected job and checks the expected results, including verified reuse and intentionally skipped platform work. Matrix job names vary with the selected scope, so the stable gate enforces their combined result. Administrators cannot bypass this CI rule.

The repository rule is recorded in `.github/rulesets/required-ci.json`. Add other integration branches there before accepting dependent PRs, and apply the same configuration in GitHub's repository rulesets. Changes to protected integration branches go through PRs; ordinary feature branches keep normal pushes.

Ordinary PRs do not build macOS/iOS apps or create signed packages. Main and manual validation build affected native products, and release PRs retain macOS release-artifact preparation. The quality gate verifies this distribution policy; it does not claim that an ordinary PR compiled on macOS. Platform failures can therefore be detected after an ordinary PR merges. Check CI on the latest commit before merging, and request manual full validation when a change needs native packaging assurance.

## Check a complete bundle

```sh
vmux build my-vmux -p local
vmux run my-vmux -p local
```

Use this when the change affects native integration, resources, or packaging. `vmux cache status` reports build storage, and `vmux cache clean` previews reclaimable caches; add `--apply` to clean them. Running apps and locked build outputs are retained. Try the behavior in that completed build. Use copied data for storage/migration changes.

Set `VMUX_DATA_DIR` to an absolute copied data root for persistence QA. Configuration lives under its `config/` directory, and Tools uses its `home/` directory for dotfile targets. Copy the relevant configuration and home files into those directories; keep them separate from live paths. This does not relocate external package managers such as Homebrew, so use managed ACP/LSP packages for isolated install and removal checks. Ordinary runs without the override keep the existing user paths. Copied managed dotfile symlinks can still point to live package sources. Before testing enable/disable, recreate or retarget only the managed links inside the copied home to the copied sources under `config/tools/dotfiles`; preserve unrelated copied files and links. A copied parent such as `home/.config` must also resolve inside the copy. Do not modify the live home to prepare QA.

## Choose how to share

A local build can be the finished result. Push to your fork if you want a durable personal branch or a reviewable PR. For an upstream contribution, keep the diff focused and explain the behavior and validation.

A distributable release has additional signing, resource, update-verification, and platform requirements. The release workflow defines the current steps and artifact names. Use your own product identity and update settings for an independently branded version; see [Make your own Vmux](customization.md).

## Preview these guides

Run the website for the branch you are editing:

```sh
vmux serve my-vmux
```

Open the URL shown in Terminal, then visit `/docs`. This starts the website development server and watches its styles while you edit the guides.

## Give a distributed build its own name

A personal code change can use the normal development build. For an independently named app, create a `.env` file at the root of your worktree:

```dotenv
VMUX_HOST_NAME=My Vmux
VMUX_HOST_NAMESPACE=com.yourname.myvmux
```

The first value is the app's display name. The second is its unique app identifier, which separates its data and service paths. Replace the example identifier with one for your product. Build and run with the same ordinary commands; Vmux reads this file from the selected worktree.

Distribution also needs your product's assets, permissions, signing, and update settings. Configure those before sharing an installer. They are separate from the first build-and-edit workflow.
