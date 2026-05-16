---
name: vmux-release
description: Use when releasing a new vmux version, cutting a vmux release, validating vmux release automation, or troubleshooting vmux release artifacts.
---

# Vmux Release Procedure

## Preconditions

- Start from the repo root, but do not edit files in the main worktree.
- Main is clean and current enough to branch from.
- All recent CI runs green.
- Apple signing secrets configured in repo Actions secrets: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_APP_PASSWORD`, `APPLE_TEAM_ID`.
- Update signing secrets configured: `VMUX_UPDATE_PUBLIC_KEY`, `VMUX_UPDATE_PRIVATE_KEY`, `VMUX_UPDATE_PRIVATE_KEY_PASSWORD`.

## Steps

1. **Categorize the upgrade.** Choose exactly one SemVer category before bumping:
   - **Patch:** bug fixes, CI/release fixes, docs, dependency updates, no user-facing behavior change.
   - **Minor:** new user-facing features, compatible behavior changes, new settings or commands.
   - **Major:** breaking changes, incompatible config/data/API changes, removed behavior, required manual migration.
   Check `git log v$LAST..HEAD` for changes since last release and map every notable change into one category. Use the highest category present.

2. **Determine next version.** Apply the category to the current version:
   - Patch: `X.Y.Z` -> `X.Y.(Z+1)`
   - Minor: `X.Y.Z` -> `X.(Y+1).0`
   - Major: `X.Y.Z` -> `(X+1).0.0`

3. **Create release worktree and bump version.**
   ```bash
   git worktree list
   git worktree add .worktrees/vmux-release-vX.Y.Z -b release/vX.Y.Z main
   cd .worktrees/vmux-release-vX.Y.Z
   git pull --rebase origin main
   # Edit Cargo.toml: workspace.package.version = "X.Y.Z"
   make lint
   make test
   git commit -am "chore(release): vX.Y.Z"
   git push -u origin release/vX.Y.Z
   ```

4. **Open PR.**
   ```bash
   gh pr create --title "chore(release): vX.Y.Z" --body "Release vX.Y.Z"
   ```

5. **Wait for CI green.** Lint + test always run; build-mac runs only if packaging-touching paths changed.

6. **Squash-merge PR to main.**
   ```bash
   gh pr merge --squash --delete-branch
   ```

7. **Watch release pipeline.**
   ```bash
   gh run watch $(gh run list --workflow=release.yml --limit 1 --json databaseId --jq '.[0].databaseId')
   ```
   Expected: ~50 min for the macOS sign + notarize + DMG build.

8. **Verify GH release.**
   ```bash
   gh release view vX.Y.Z
   ```
   Assets present: `Vmux_X.Y.Z_aarch64.dmg`, `vmux-vX.Y.Z-aarch64-apple-darwin.tar.gz`, `Vmux-vX.Y.Z-aarch64-apple-darwin.app.tar.gz`, `Vmux-vX.Y.Z-aarch64-apple-darwin.app.tar.gz.sig`.

9. **Verify auto-commit on main.**
   ```bash
   git pull --rebase origin main
   git log -1 --stat
   ```
   Expect a commit `chore(release): update cask and update manifest to X.Y.Z` touching `Casks/vmux.rb` and `website/public/updates.json`.

10. **Verify website redeploy.** Wait ~5 min for `deploy-website.yml`, then:
   ```bash
   curl -fsSL https://vmux.ai/updates.json | jq .version
   ```
   Should print `"vX.Y.Z"`.

11. **Smoke test.** On a clean macOS machine or VM:
    ```bash
    brew tap vmux-ai/vmux https://github.com/vmux-ai/vmux
    brew install --cask vmux
    # or, for existing installs:
    brew upgrade --cask vmux
    open -a Vmux
    ```
    Confirm app launches and `About` shows the new version.

12. **(Optional) Test auto-update.** Install previous version, launch, wait for poll interval (default 1h). App should download + replace itself with the new version on next launch.

## If something goes wrong

- **Release pipeline fails partway:** check `gh run view --log-failed`. Common: notarization timeout (Apple side, retry the run), missing secret (re-add).
- **detect-version says version unchanged after cask + manifest auto-commit:** expected. The auto-commit does not change `Cargo.toml`, so release should skip.
- **detect-version says version unchanged after the release bump merge:** unexpected. The merged release commit did not change `Cargo.toml` version. Confirm the bump landed; check `git show main:Cargo.toml | head -10`.
- **Tag already exists:** release.yml resumes if the existing `vX.Y.Z` tag points at the release SHA. It aborts if the tag points elsewhere. Fix by bumping version higher and pushing a new commit to main, or only delete the bad tag if you are certain it is safe.
- **Cask commit conflicts:** rare; another commit landed on main between release start and cask push. The workflow does `git pull --rebase origin main` before push; if that fails, re-run the workflow.
- **vmux.ai/updates.json shows old version:** `deploy-website.yml` didn't fire. Check workflow runs; manually re-run via `gh workflow run deploy-website.yml`.
