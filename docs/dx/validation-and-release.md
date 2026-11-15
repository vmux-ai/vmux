# Validation and release

Use the smallest check that exercises a change while editing. The pre-push gate and CI provide the broader safety net before it lands.

## The inner loop

```sh
vmux internal cargo -- test --locked -p vmux_layout --lib
vmux internal cargo -- clippy --locked -p vmux_layout --all-targets
cargo fmt --all -- --check
```

Select the actual feature and test; `vmux_layout` is an example. Run one build/test invocation at a time per workspace. If a build is needed, use its result instead of paying for a compile check first. Check exit status and the new artifact before reporting success.

Use development builds for iteration and local packages for native integration, permissions, or packaging QA. People launch and test the app; agents build, inspect logs, and resolve failures.

## Executable policy

```sh
vmux internal pre-push
```

The policy target checks ECS ownership, adapter provenance, and frozen migration debt. Conventions checks repository-specific source rules. fmt owns formatting, Clippy owns standard Rust diagnostics, and feature tests own behavior. The check implementations and their Cargo targets are the source of truth for each rule.

When a check rejects new code, improve the code or document its actual bounded interface. Do not refresh legacy fingerprints or add broad exceptions. Detailed rules and regression fixtures live with the checker; these guides explain their architectural reason.

The pre-push hook runs CLI/DX regressions, repository policy, formatting, and diff validation. Install it in every feature worktree. CI treats failed or unexpectedly skipped required checks as failure.

## CI and integration

CI selects affected crates and consumers from source and Cargo metadata. Shared contracts, manifests, toolchains, build inputs, or uncertain ownership require broader validation. Native checks and packaging follow affected platform/build boundaries.

CI reuses the verified development CLI within a run and runs independent Rust/lean checks in parallel. Selected native tooling checks share their preparation runner. Branch validation is reusable only for an exact head, captured base, merged tree, and compatible execution definition. Main and release validation retain their required package/startup checks. Check the latest head and all applicable results before merging.

Describe behavior, validation, and required manual QA in the PR. Rebase onto a fetched target when it advances, keep the diff scoped, and respect branch leases. After merge, remove only landed, unused worktrees and compile caches; retain active work and prepared QA artifacts until their owner releases them.

## Distribution

Local packages exercise native integration without publishing a release. Release preparation adds pinned toolchain/browser provenance, product identity, signing, packaging, update verification, and required notarization.

Workflow and command implementations define artifact names and exact steps. Build CLI, application binaries, metadata, and resources from the same accepted product inputs. Version persisted and wire contracts with their owner, and identify breaking changes for consumers.

Signed standalone CLI updates and checkout development refresh have different owners. Release updates verify signed artifacts; source refresh follows the selected checkout. Neither path replaces another runtime or imports another product's data.
