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
- iOS App Store secrets configured: `APPLE_IOS_SIGNING_IDENTITY`, `APPLE_IOS_CERTIFICATE`, `APPLE_IOS_CERTIFICATE_PASSWORD`, `APPLE_IOS_PROVISIONING_PROFILE`. Distinct from the macOS pair above — `APPLE_CERTIFICATE` is a Developer ID certificate and cannot sign for the App Store, so this needs an Apple Distribution certificate and a provisioning profile for `ai.vmux.mobile`. `APPLE_ID`, `APPLE_APP_PASSWORD` and `APPLE_TEAM_ID` are shared with macOS.
- App Store Connect API key configured for automated submission: `APPLE_ASC_KEY_ID`, `APPLE_ASC_ISSUER_ID`, `APPLE_ASC_PRIVATE_KEY`. Optional — without them the build still reaches TestFlight and only the submit-for-review step is skipped.
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

4. **Open one release PR.**
   ```bash
   gh pr create --title "chore(release): vX.Y.Z" --body "Release vX.Y.Z"
   ```

5. **Wait for draft assets.** The first CI run builds, signs, notarizes, smoke-tests, and uploads four assets to a draft GitHub release. Its final `Require release metadata commit` step is expected to fail until the metadata commit exists.

6. **Update metadata on the same PR branch.**
   ```bash
   ./scripts/update-release-metadata.sh
   git diff --check
   ruby -c Casks/vmux.rb
   jq empty website/public/updates.json
   git add Casks/vmux.rb website/public/updates.json
   git commit -m "chore(release): update vX.Y.Z metadata"
   git push
   ```
   This reads the final draft assets, updates the Homebrew checksum and updater signature, and keeps the release to one PR with two commits. Never merge a version-only release PR.

7. **Wait for the second CI run and review.** The idempotency guard reuses the draft assets instead of rebuilding them. Confirm every required check is green and every review thread is handled.

8. **Squash-merge the single PR to main.**
   ```bash
   gh pr merge --squash --delete-branch
   ```

9. **Watch release pipeline.**
   ```bash
   gh run watch $(gh run list --workflow=release.yml --limit 1 --json databaseId --jq '.[0].databaseId')
   ```
   The expensive macOS build already ran on the PR. The main release workflow verifies committed metadata, retargets the tag to the merge commit, publishes the draft, and dispatches the website deploy.

10. **Verify GH release.**
   ```bash
   gh release view vX.Y.Z
   ```
   Assets present: `Vmux_X.Y.Z_aarch64.dmg`, `vmux-vX.Y.Z-aarch64-apple-darwin.tar.gz`, `Vmux-vX.Y.Z-aarch64-apple-darwin.app.tar.gz`, `Vmux-vX.Y.Z-aarch64-apple-darwin.app.tar.gz.sig`.

11. **Verify merged metadata on main.**
   ```bash
   git fetch origin main
   git show origin/main:Casks/vmux.rb
   git show origin/main:website/public/updates.json
   ```
   Both files must reference `vX.Y.Z`.

12. **Verify website redeploy.** Wait for `deploy-website.yml`, then:
   ```bash
   curl -fsSL https://vmux.ai/updates.json | jq .version
   ```
   Should print `"vX.Y.Z"`.

13. **Smoke test.** On a clean macOS machine or VM:
    ```bash
    brew tap vmux-ai/vmux https://github.com/vmux-ai/vmux
    brew install --cask vmux
    # or, for existing installs:
    brew upgrade --cask vmux
    open -a Vmux
    ```
    Confirm app launches and `About` shows the new version.

14. **(Optional) Test auto-update.** Install previous version, launch, wait for poll interval (default 1h). App should download + replace itself with the new version on next launch.

## iOS

The same version bump releases the iOS app. Two jobs in `release.yml` run off `detect-version`, alongside `publish`:

- **`Upload iOS to TestFlight`** — builds `aarch64-apple-ios`, injects the icon, launch screen and privacy manifest, signs with the Apple Distribution certificate, packages an `.ipa` and uploads it. `CFBundleVersion` is the workflow run number, so re-uploads never collide.
- **`Submit iOS for review`** — waits for Apple to finish processing, attaches the build to the `X.Y.Z` App Store version, and submits it. Skips with a notice when the App Store Connect API key secrets are absent, so the build still reaches TestFlight either way.

**Neither blocks the desktop release.** `publish` needs only `detect-version`, so a broken signing certificate cannot hold back a DMG that already built and notarized. The flip side is that a red iOS job is easy to miss, so check it explicitly:

```bash
gh run view $(gh run list --workflow=release.yml --limit 1 --json databaseId --jq '.[0].databaseId') \
  --json jobs --jq '.jobs[] | select(.name | test("iOS")) | "\(.name): \(.conclusion)"'
```

15. **Confirm the build reached TestFlight.** App Store Connect → TestFlight → iOS builds. The new build is `X.Y.Z (<run number>)`. Processing usually takes 5–15 minutes; the submit job waits up to 30.

16. **Confirm the submission.** The `X.Y.Z` version should read *Waiting for Review*. If the submit job skipped, submit it there by hand.

17. **Watch for the review outcome.** Typically 24–48h. A rejection arrives in App Store Connect, not in CI — nothing in this repo will tell you.

### iOS gotchas

- **`CFBundleShortVersionString` is the workspace version**, shared with the desktop. Bumping for a desktop-only fix still moves the iOS marketing version and, once the API key is configured, submits a new build for review. Consider whether the mobile app actually changed before bumping.
- **Submission fails until the App Store listing is complete.** Screenshots, description, age rating and the privacy questionnaire have to exist for that version first.
- **One version in review at a time.** Bumping again while `X.Y.Z` is *Waiting for Review* fails the submit job; cancel the earlier submission first.
- **Export compliance is already answered.** `ITSAppUsesNonExemptEncryption = false` ships in the bundle, so there is no per-upload prompt.

## If something goes wrong

- **First CI fails only at `Require release metadata commit`:** expected. Run `./scripts/update-release-metadata.sh`, commit both metadata files to the same branch, and push.
- **Metadata helper says release or assets are missing:** the first macOS CI job has not finished creating the draft release. Wait for it, then retry.
- **Second CI rebuilds the macOS assets:** the cask URL does not match the draft release. Re-run the metadata helper and inspect the diff.
- **Release pipeline fails metadata verification:** the release PR was merged without matching cask/updater metadata. Do not publish manually; repair the release process before retrying.
- **Release pipeline fails partway:** check `gh run view --log-failed`. Common: notarization timeout (Apple side, retry the PR CI run), missing secret (re-add).
- **detect-version says version unchanged after the release bump merge:** unexpected. The merged release commit did not change `Cargo.toml` version. Confirm the bump landed; check `git show main:Cargo.toml | head -10`.
- **Tag already exists:** release.yml resumes if the existing `vX.Y.Z` tag points at the release SHA. It aborts if the tag points elsewhere. Fix by bumping version higher and pushing a new commit to main, or only delete the bad tag if you are certain it is safe.
- **vmux.ai/updates.json shows old version:** `deploy-website.yml` didn't fire. Check workflow runs; manually re-run via `gh workflow run deploy-website.yml`.
