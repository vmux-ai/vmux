# Release Flow Revision Design

## Goal

Replace the current three-workflow release pipeline (`ci.yml` → `tag-release.yml` → `release.yml`) with a faster, more reliable two-workflow flow that:

- Skips duplicate `make lint` / `make test` work in the release run.
- Eliminates the Homebrew-cask auto-commit re-triggering CI.
- Removes the brittle `workflow_run` dependency between CI and tag creation.
- Hosts the auto-update manifest at `https://vmux.ai/updates.json` (a branded, stable URL on the vmux.ai domain).
- Unifies the macOS build path so CI and release call the same script.

## Non-goals

- Multi-platform builds (Linux, x86_64-apple-darwin) — out of scope for this revision.
- Multi-channel updates (beta/stable) — out of scope.
- Backward compatibility for v0.0.2 installs — there are no users on v0.0.2.

## Current flow (for reference)

```
PR `chore(release): vX.Y.Z` (bumps Cargo.toml workspace.package.version)
        ↓ merge to main
ci.yml                   triggers on push:main; runs lint, test, website-if-touched, build-mac always
tag-release.yml          triggers on workflow_run of CI completed=success on main;
                         diffs Cargo.toml HEAD vs HEAD^; if version changed, creates+pushes vX.Y.Z tag
release.yml              triggers on tag push v*; re-runs make lint && make test;
                         builds, signs, notarizes, packages DMG; creates GH release;
                         commits Casks/vmux.rb update back to main → re-triggers CI
```

End-to-end latency: ~2.5 hours (CI ~1h15m + Tag-Release ~17s + Release ~1h+).

## Revised flow

```
PR `chore(release): vX.Y.Z` (bumps Cargo.toml workspace.package.version)
        ↓ merge to main
ci.yml                   triggers on push:main with paths-ignore [Casks/**, website/public/updates.json]
                         lint + test always; build-mac only on packaging-touching paths
release.yml              triggers on push:main; first job detects Cargo.toml version diff
                         vs HEAD^; if changed, runs build job:
                           - scripts/build-mac-release.sh (signs + notarizes; secrets exported)
                           - create+push tag vX.Y.Z (informational only)
                           - gh release create with DMG, app tarball, .sig
                           - write website/public/updates.json (auto-update manifest)
                           - update Casks/vmux.rb
                           - single commit covering both files; push to main
deploy-website.yml       triggers on push:main paths website/**; rebuilds + publishes
                         https://vmux.ai/updates.json
```

End-to-end latency target: ~1 hour (lint+test parallel with release build; deploy-website ~5min after release commit).

## Architecture decisions

### Trigger model

Use **push-to-main + version-diff detection** in a single workflow. Replaces the brittle `workflow_run` chain with a self-contained release pipeline. The detect-version job is fast (<10s) and cheap; it gates the expensive build job.

Rejected alternatives:
- **Tag-driven** (`git tag && git push`): adds a manual step and risks human error in version naming.
- **PR label**: requires label discipline; releases-as-PRs already self-document via title `chore(release): vX.Y.Z`.
- **workflow_dispatch with version input**: too manual; defeats the goal of "merge → release".

### CI / release relationship

The two workflows run **independently in parallel** on the same push. Each is responsible for its own checks:

- `ci.yml` validates correctness (lint, test) regardless of whether a release is happening.
- `release.yml` validates packaging (build, sign, notarize) only when a release is happening.

`release.yml` does **not** wait for `ci.yml`. If lint or test fails after a release commit lands, that's a bug to fix forward — but the release artifact has already been built and validated independently. This is acceptable because version bumps are rare deliberate events; we don't gate them on every CI signal.

Rejected alternative: have `release.yml` wait for `ci.yml`. This recreates the workflow_run brittleness we are trying to remove.

### Cask + manifest auto-commit without CI loop

The release job commits two files back to main: `Casks/vmux.rb` (Homebrew cask update) and `website/public/updates.json` (auto-update manifest). To prevent this commit from re-triggering CI:

- `ci.yml`'s `push` trigger gets `paths-ignore: ['Casks/**', 'website/public/updates.json']`.
- The auto-commit only touches those two files, so CI is not triggered.
- `pull_request` trigger keeps no paths-ignore — real PRs editing those files still get full CI.
- `deploy-website.yml`'s `paths: ['website/**']` filter still matches `website/public/updates.json`, so it redeploys.

Rejected alternative: `[skip ci]` in commit message. Skips ALL workflows for that commit, including `deploy-website.yml`, so the manifest never goes live.

### Unified macOS build script

CI and release use the same entry point: `scripts/build-mac-release.sh`. The script always runs sign + notarize (no `APPLE_SIGNING_IDENTITY` guard). Implications:

- Signing secrets must be exported in CI's `build-mac` job too.
- Fork PRs cannot run `build-mac` (no secret access). Acceptable: this project does not currently accept fork PRs.
- Every CI `build-mac` run pays ~10 min Apple notary wait. Mitigated by paths-filter (see next section).

### CI `build-mac` paths filter

`build-mac` runs only when the PR touches packaging-related paths:
- `scripts/**`
- `patches/**`
- `crates/vmux_desktop/Cargo.toml`
- `Cargo.lock`
- `.github/workflows/release.yml`
- `.github/workflows/ci.yml`

Implementation: extend the existing `changes` job with a `packaging` filter, gate `build-mac` on `needs.changes.outputs.packaging == 'true'`. Most PRs (Rust source-only) skip the macOS build entirely.

### Auto-update endpoint

App polls `https://vmux.ai/updates.json` (changed from the current `https://github.com/vmux-ai/vmux/releases/latest/download/update-manifest.json`).

- Branded URL on the vmux.ai domain.
- Independent of GitHub Releases API and rate limits.
- Single source of truth (manifest committed to repo at `website/public/updates.json`, not a GH release asset).
- Future-proof: easy to add channels (`updates-beta.json`) or different platforms later.

The `update-manifest.json` is no longer attached to GH releases. The `.sig` file remains attached for verification.

## File-level changes

### Modified

| File | Change |
|------|--------|
| `crates/vmux_desktop/src/updater.rs` | Line 7-8: `DEFAULT_ENDPOINT` → `"https://vmux.ai/updates.json"` |
| `.github/workflows/ci.yml` | Add `paths-ignore` on `push` trigger. Add `packaging` filter to `changes` job. Gate `build-mac` on `needs.changes.outputs.packaging`. Replace inline build/sign/notarize steps with `./scripts/build-mac-release.sh`. Export signing secrets to `build-mac`. |
| `.github/workflows/release.yml` | Trigger: `on: push: branches: [main]` (drop `tags: v*`). Add `detect-version` job: diff `Cargo.toml` HEAD vs HEAD^, output `should_release` + `version` + `tag`, verify tag absence. `release-macos` job becomes `needs: detect-version` and gated on `outputs.should_release == 'true'`. Drop `make lint` / `make test` steps. Keep `./scripts/build-mac-release.sh`. After `gh release create`: write `website/public/updates.json`, update `Casks/vmux.rb`, single commit, push. Drop `update-manifest.json` from `gh release create` asset list. |

### Deleted

| File | Reason |
|------|--------|
| `.github/workflows/tag-release.yml` | Functionality merged into `release.yml`'s `detect-version` job. |

### Unchanged

| File | Reason |
|------|--------|
| `scripts/build-mac-release.sh` | No `APPLE_SIGNING_IDENTITY` guard added (per design call). |
| `.github/workflows/deploy-website.yml` | Existing `paths: ['website/**']` already catches `website/public/updates.json`. |
| `Casks/vmux.rb` | `release.yml` already updates it; logic preserved. |

## New files

### `.claude/skills/vmux-release/SKILL.md`

Project-local skill that codifies the manual release procedure (run by a human or by Claude when asked to release). Contents:

```markdown
---
name: vmux-release
description: Use when releasing a new vmux version (e.g., "release v0.0.3", "cut a new release"). Bumps Cargo.toml version on a release branch, opens PR, monitors CI + release pipeline, verifies artifacts.
---

# Vmux Release Procedure

## Preconditions

- On `main`, working tree clean.
- All recent CI runs green.
- Apple signing secrets configured in repo Actions secrets (APPLE_CERTIFICATE, APPLE_CERTIFICATE_PASSWORD, APPLE_SIGNING_IDENTITY, APPLE_ID, APPLE_APP_PASSWORD, APPLE_TEAM_ID).
- Update signing secrets configured (VMUX_UPDATE_PUBLIC_KEY, VMUX_UPDATE_PRIVATE_KEY, VMUX_UPDATE_PRIVATE_KEY_PASSWORD).

## Steps

1. **Determine next version.** Use semver. Patch bump for fixes, minor for features, major for breaking. Check `git log v$LAST..HEAD` for changes since last release.

2. **Create release branch and bump version.**
   ```bash
   git checkout main && git pull
   git checkout -b release/vX.Y.Z
   # Edit Cargo.toml: workspace.package.version = "X.Y.Z"
   git commit -am "chore(release): vX.Y.Z"
   git push -u origin release/vX.Y.Z
   ```

3. **Open PR.**
   ```bash
   gh pr create --title "chore(release): vX.Y.Z" --body "Release vX.Y.Z"
   ```

4. **Wait for CI green.** Lint + test always run; build-mac runs if packaging-touching paths changed.

5. **Squash-merge PR to main.**
   ```bash
   gh pr merge --squash --delete-branch
   ```

6. **Watch release pipeline.**
   ```bash
   gh run watch $(gh run list --workflow=release.yml --limit 1 --json databaseId --jq '.[0].databaseId')
   ```
   Expected: ~50min for the macOS sign + notarize + DMG build.

7. **Verify GH release.**
   ```bash
   gh release view vX.Y.Z
   ```
   Assets present: `Vmux_X.Y.Z_aarch64.dmg`, `vmux-vX.Y.Z-aarch64-apple-darwin.tar.gz`, `Vmux-vX.Y.Z-aarch64-apple-darwin.app.tar.gz`, `Vmux-vX.Y.Z-aarch64-apple-darwin.app.tar.gz.sig`.

8. **Verify auto-commit on main.**
   ```bash
   git pull
   git log -1 --stat  # expect: Casks/vmux.rb + website/public/updates.json
   ```

9. **Verify website redeploy.** Wait ~5 min for `deploy-website.yml`, then:
   ```bash
   curl -fsSL https://vmux.ai/updates.json | jq .version
   ```
   Should print `"vX.Y.Z"`.

10. **Smoke test.** On a clean macOS machine or VM:
    ```bash
    brew upgrade --cask vmux  # or fresh install: brew install --cask vmux
    open -a Vmux
    ```
    Confirm app launches and `About` shows the new version.

11. **(Optional) Test auto-update.** Install previous version, launch, wait for poll interval (default 1h, configurable in `crates/vmux_desktop/src/updater.rs`). App should download + replace itself with the new version on next launch.

## If something goes wrong

- **Release pipeline fails partway:** check `gh run view --log-failed`. Common: notarization timeout (Apple side, retry the run), missing secret (re-add).
- **Tag already exists:** the detect-version job aborts. Fix: bump version higher and push a new commit to main.
- **Cask commit conflicts:** rare; another commit landed on main between release start and cask push. Fix: re-run the workflow.
- **vmux.ai/updates.json shows old version:** deploy-website.yml didn't fire. Check workflow runs; manually re-run if needed.
```

### `website/public/updates.json` (will be created by first release run)

Not pre-seeded. The v0.0.3 release run will create it. Until then, `https://vmux.ai/updates.json` returns 404, which is acceptable because no app instance polls that URL until v0.0.3 ships with the new `DEFAULT_ENDPOINT`.

## Testing plan

The whole flow is exercised end-to-end by releasing v0.0.3 via the new pipeline:

1. Land all design changes via a normal feature PR (`feat(ci): unified release pipeline`).
2. Verify the merge of that PR runs `ci.yml` but does NOT trigger `release.yml` (no version bump → detect-version short-circuits).
3. Run the `vmux-release` skill for v0.0.3:
   - `chore(release): v0.0.3` PR
   - Merge
   - Confirm `release.yml` detects version bump, builds, releases
   - Confirm `Casks/vmux.rb` and `website/public/updates.json` land in main
   - Confirm `deploy-website.yml` publishes the manifest
   - Confirm `https://vmux.ai/updates.json` returns the v0.0.3 manifest
   - Confirm a v0.0.3 install (via `brew install --cask vmux`) works
4. Document any surprises in the skill SKILL.md.

## Open risks

- **Race between release.yml and CI.** Both run on the same push. If CI fails (lint/test broke), the release artifact is still built. Acceptable — release commits are deliberate and pre-validated by the release-PR's CI run. Worst case: release ships with broken lint/test that wasn't caught pre-merge (rare; release PRs are usually trivial version bumps).
- **Cask + manifest commit race.** If a developer pushes to main while `release.yml` is mid-build, the cask commit may need a rebase. Mitigation: the auto-commit step uses `git pull --rebase` before `git push`. If the rebase conflicts (unlikely; auto-commit only touches two specific files), the workflow fails loudly and a human intervenes.
- **Notarization flakiness.** Apple's notary service can hang or fail intermittently. The current flow has no retry. Out of scope for this revision; address if it becomes a recurring problem.
