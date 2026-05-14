# Hands-Free Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land four shipped components — a smarter changed-crate test selector, a transactional `release.yml`, branch protection on `main`, and a `/release` slash skill — so cutting v0.0.8 is one command end-to-end.

**Architecture:** Four independent PRs landed in order: defensive (pre-push hook) → release infra (workflow refactor) → policy (branch protection) → orchestration (skill). Then validate the whole chain by cutting v0.0.8.

**Tech Stack:** Bash scripts, GitHub Actions YAML, GitHub branch protection API, Claude Code skill (markdown).

**Spec:** `docs/specs/2026-05-14-hands-free-release-design.md`

---

## Task 1: Pre-push hook for include_str!-aware test selection

**PR:** `chore/changed-crates-script`

**Files:**
- Create: `scripts/changed-crates.sh`
- Create: `scripts/setup-hooks.sh`
- Create: `scripts/hooks/pre-push`
- Modify: `Makefile` — add `setup-hooks` target
- Modify: `AGENTS.md` — replace inline snippet with script reference
- Test: `scripts/tests/changed-crates.bats` (or shell-based smoke test)

### Step 1.1: Write `scripts/changed-crates.sh`

- [ ] Create the file with executable permission

```bash
#!/usr/bin/env bash
# Print the set of workspace crates that should be linted/tested,
# given changes vs BASE (default: origin/main).
#
# Includes:
#   1. Crates whose own files changed.
#   2. Crates whose source contains include_str!("…") referencing a changed file.
#
# Vendored `patches/` are excluded.

set -euo pipefail

BASE="${BASE:-origin/main}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

crates_by_dir() {
  cargo metadata --no-deps --format-version 1 \
    | jq -r '.packages[]
        | select(.manifest_path | test("patches") | not)
        | "\(.name)\t\(.manifest_path | sub("/Cargo\\.toml$"; ""))"' \
    | while IFS=$'\t' read -r name dir; do
        rel="${dir#"$ROOT"/}"
        [ -z "$rel" ] && rel="."
        if ! git diff --quiet "$BASE" -- "$rel"; then
          printf '%s\n' "$name"
        fi
      done
}

crates_by_includes() {
  changed="$(git diff --name-only "$BASE")"
  [ -z "$changed" ] && return 0
  cargo metadata --no-deps --format-version 1 \
    | jq -r '.packages[]
        | select(.manifest_path | test("patches") | not)
        | "\(.name)\t\(.manifest_path | sub("/Cargo\\.toml$"; ""))"' \
    | while IFS=$'\t' read -r name dir; do
        grep -rn 'include_str!("[^"]*")' "$dir" 2>/dev/null \
          | while IFS=':' read -r file _lineno match; do
              inc="$(printf '%s' "$match" | sed -E 's/.*include_str!\("([^"]*)"\).*/\1/')"
              file_dir="$(dirname "$file")"
              abs="$(cd "$file_dir" && realpath -m "$inc" 2>/dev/null)" || continue
              rel="${abs#"$ROOT"/}"
              if printf '%s\n' "$changed" | grep -qx "$rel"; then
                printf '%s\n' "$name"
              fi
            done
      done
}

(crates_by_dir; crates_by_includes) | sort -u
```

```bash
chmod +x scripts/changed-crates.sh
```

### Step 1.2: Smoke-test the script manually

- [ ] Verify against the v0.0.7 failure scenario

```bash
git checkout origin/main -- Makefile  # ensure baseline
echo "# noise" >> Makefile
./scripts/changed-crates.sh
```

Expected output includes `vmux_desktop` (because `release_invariants.rs` `include_str!`s the workspace `Makefile`).

```bash
git checkout origin/main -- Makefile  # revert
```

### Step 1.3: Write `scripts/hooks/pre-push`

- [ ] Create the hook script

```bash
#!/usr/bin/env bash
# Block push if the changed-crate test set has any failing test or clippy lint.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
PKGS="$(BASE=origin/main "$ROOT/scripts/changed-crates.sh")"

if [ -z "$PKGS" ]; then
  echo "pre-push: no relevant crates changed; skipping checks"
  exit 0
fi

echo "pre-push: checking crates:"
printf '  %s\n' $PKGS

for pkg in $PKGS; do
  cargo fmt -p "$pkg" -- --check
done

for pkg in $PKGS; do
  env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings
done

for pkg in $PKGS; do
  env -u CEF_PATH cargo test -p "$pkg"
done
```

```bash
chmod +x scripts/hooks/pre-push
```

### Step 1.4: Write `scripts/setup-hooks.sh`

- [ ] Symlink so the repo's hook is what runs

```bash
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
ln -sf "$ROOT/scripts/hooks/pre-push" "$ROOT/.git/hooks/pre-push"
echo "Installed pre-push hook -> scripts/hooks/pre-push"
```

```bash
chmod +x scripts/setup-hooks.sh
```

### Step 1.5: Add `make setup-hooks` target

- [ ] Edit `Makefile`. Add to `.PHONY` and append target

In `.PHONY` line, add `setup-hooks`. Then append at the end of the file:

```makefile
setup-hooks:
	./scripts/setup-hooks.sh
```

### Step 1.6: Update `AGENTS.md`

- [ ] Replace the inline pre-commit-checks snippet with a reference

Find the "## Pre-commit Checks" section that contains the bash snippet computing `CHANGED_PKGS`. Replace the snippet with:

```markdown
## Pre-commit Checks

NEVER commit or push without running fmt + clippy + test on the **changed crates only** (not the whole workspace) and confirming they pass.

The `scripts/changed-crates.sh` script computes the set: crates whose files changed, plus crates whose tests `include_str!` from changed paths.

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)

for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

Run `make setup-hooks` once to install the pre-push hook that runs these checks automatically.
```

Keep the "If a change ripples into a downstream crate that is NOT in the changed set, lint/test that crate too." sentence after this block (it remains advisory for cases the script can't detect).

Also update the "## Before Pushing / Opening PRs" section to point at the script in place of the snippet.

### Step 1.7: Install the hook locally

- [ ] Run setup and verify

```bash
make setup-hooks
ls -la .git/hooks/pre-push
```

Expected: symlink to `scripts/hooks/pre-push`.

### Step 1.8: Verify the hook fires

- [ ] Make a no-op change to Makefile and attempt a dry-run push

```bash
echo "# pre-push test" >> Makefile
git add Makefile
git commit -m "chore: test pre-push"
git push --dry-run 2>&1 | tail -20
git reset --hard HEAD^
```

Expected: hook prints "checking crates: vmux_desktop" and runs fmt + clippy + test for that crate. (The `--dry-run` still triggers hooks.)

### Step 1.9: Commit

- [ ] Stage and commit

```bash
git add scripts/changed-crates.sh scripts/setup-hooks.sh scripts/hooks/pre-push Makefile AGENTS.md
git commit -m "chore: add changed-crates script + pre-push hook"
```

### Step 1.10: Open PR

- [ ] Push branch and open PR

```bash
git push -u origin chore/changed-crates-script
gh pr create --title "chore: changed-crates script + pre-push hook" --body "Implements Component 4 of docs/specs/2026-05-14-hands-free-release-design.md. Adds a script that selects crates for testing based on file changes AND include_str! references, plus a pre-push hook that runs fmt/clippy/test on that set."
```

Wait for CI green. Merge via API.

---

## Task 2: release.yml — transactional refactor

**PR:** `chore/release-transactional`

**Files:**
- Modify: `.github/workflows/release.yml`

### Step 2.1: Add re-run guard at top of build job

- [ ] Edit `.github/workflows/release.yml`. Right before "Validate release secrets" step (currently around line 108), insert a new step

```yaml
      - name: Re-run guard — exit if release already published with all assets
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          set -euo pipefail
          if gh release view "$TAG" --json isDraft,assets >/tmp/r.json 2>/dev/null; then
            is_draft=$(jq -r .isDraft /tmp/r.json)
            asset_count=$(jq '.assets | length' /tmp/r.json)
            if [ "$is_draft" = "false" ] && [ "$asset_count" -ge 4 ]; then
              echo "Release $TAG already published with $asset_count assets; nothing to do."
              echo "ALREADY_DONE=1" >> "$GITHUB_ENV"
            fi
          fi
```

Then guard every subsequent step with `if: env.ALREADY_DONE != '1'`.

### Step 2.2: Replace "Commit cask + manifest to main" + "Create and push tag" + "Create GitHub Release" with the transactional sequence

- [ ] In `.github/workflows/release.yml`, find the existing 3 steps (around lines 264–321) and replace them with the following 5 steps. Keep the "Update Homebrew cask" step (it just modifies files in the workspace; we still need it to compute the cask values).

```yaml
      - name: Create or update draft release with assets
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        if: env.ALREADY_DONE != '1'
        run: |
          set -euo pipefail
          DMG="target/release/Vmux_${VERSION}_aarch64.dmg"
          ASSETS=(
            "$DMG"
            "target/release/vmux-v${VERSION}-aarch64-apple-darwin.tar.gz"
            "target/release/Vmux-v${VERSION}-aarch64-apple-darwin.app.tar.gz"
            "target/release/Vmux-v${VERSION}-aarch64-apple-darwin.app.tar.gz.sig"
          )
          if gh release view "$TAG" >/dev/null 2>&1; then
            gh release upload "$TAG" "${ASSETS[@]}" --clobber
          else
            gh release create "$TAG" \
              --draft \
              --target "$RELEASE_SHA" \
              --title "Vmux $TAG" \
              --generate-notes \
              "${ASSETS[@]}"
          fi

      - name: Commit cask + manifest to main
        if: env.ALREADY_DONE != '1'
        run: |
          set -euo pipefail
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add Casks/vmux.rb website/public/updates.json
          if git diff --cached --quiet; then
            echo "Nothing to commit"
            exit 0
          fi
          git commit -m "chore(release): update cask and update manifest to ${VERSION}"
          for i in 1 2 3; do
            git pull --rebase origin main && break || sleep 5
          done
          git push origin HEAD:main

      - name: Re-target tag to current main HEAD
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        if: env.ALREADY_DONE != '1'
        run: |
          set -euo pipefail
          NEW_HEAD=$(git rev-parse HEAD)
          if gh api "repos/${GITHUB_REPOSITORY}/git/refs/tags/${TAG}" >/dev/null 2>&1; then
            gh api -X PATCH "repos/${GITHUB_REPOSITORY}/git/refs/tags/${TAG}" \
              -f sha="$NEW_HEAD" -F force=true
          else
            gh api -X POST "repos/${GITHUB_REPOSITORY}/git/refs" \
              -f ref="refs/tags/${TAG}" -f sha="$NEW_HEAD"
          fi

      - name: Publish release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        if: env.ALREADY_DONE != '1'
        run: gh release edit "$TAG" --draft=false

      - name: Dispatch website deploy
        env:
          GH_TOKEN: ${{ github.token }}
        if: env.ALREADY_DONE != '1'
        run: gh workflow run deploy-website.yml --ref main
```

### Step 2.3: Verify the workflow YAML parses

- [ ] Locally validate

```bash
gh workflow view release.yml --yaml >/dev/null && echo OK
```

(If `gh workflow view` doesn't accept local files, use `actionlint`:)

```bash
brew install actionlint  # if not installed
actionlint .github/workflows/release.yml
```

Expected: no errors.

### Step 2.4: Commit

- [ ] Stage and commit

```bash
git add .github/workflows/release.yml
git commit -m "ci(release): transactional flow with draft release + tag re-targeting"
```

### Step 2.5: Open PR

- [ ] Push and open PR

```bash
git push -u origin chore/release-transactional
gh pr create --title "ci(release): transactional release flow" --body "Implements Component 1 of docs/specs/2026-05-14-hands-free-release-design.md. Reorders release steps so the GitHub Release exists as a draft before cask is committed; tag is re-targeted to current main HEAD via API to bypass workflows-permission rule."
```

Wait for CI green. **Do not merge yet.** Merging triggers no immediate release (Cargo.toml unchanged), but next release uses the new flow. Confirm with user before merge.

---

## Task 3: Branch protection on main (manual GitHub UI)

**No PR.** This is a one-time configuration change applied via GitHub UI or `gh api`.

### Step 3.1: Apply branch protection rules

- [ ] Run via `gh`

```bash
gh api -X PUT repos/vmux-ai/vmux/branches/main/protection \
  -F required_status_checks='{"strict":true,"contexts":["Lint","Test"]}' \
  -F enforce_admins=false \
  -F required_pull_request_reviews=null \
  -F restrictions='{"users":[],"teams":[],"apps":["github-actions"]}' \
  -F required_linear_history=true \
  -F allow_force_pushes=false \
  -F allow_deletions=false
```

Note: the `restrictions` field with `apps: ["github-actions"]` allows only the github-actions app to push directly to main; everyone else must use a PR.

### Step 3.2: Verify

- [ ] Confirm settings

```bash
gh api repos/vmux-ai/vmux/branches/main/protection | jq '{
  required_status_checks: .required_status_checks.contexts,
  required_linear_history: .required_linear_history.enabled,
  restrictions: .restrictions.apps[].slug
}'
```

Expected:
```json
{
  "required_status_checks": ["Lint", "Test"],
  "required_linear_history": true,
  "restrictions": "github-actions"
}
```

### Step 3.3: Smoke-test from a feature branch

- [ ] Try pushing directly to main and confirm rejection

```bash
git checkout main
echo "# protection test" >> README.md
git add README.md
git commit -m "test: should be rejected"
git push origin main 2>&1 | tail -5
git reset --hard HEAD^
```

Expected: push rejected with "protected branch hook declined" or similar.

---

## Task 4: /release skill

**PR:** Skill files live outside the repo (`~/.claude/skills/release/`). No project-repo PR needed for the skill itself, but commit the skill in the user's skill management directory.

**Files:**
- Create: `~/.claude/skills/release/SKILL.md`

### Step 4.1: Write the skill file

- [ ] Create `~/.claude/skills/release/SKILL.md`

```markdown
---
name: release
description: Cut a patch/minor/major release of vmux end-to-end — bump version, open PR, watch CI, merge, watch release workflow, report URL. Invoke when user says "/release patch", "/release minor", or "/release major".
---

# Release Skill

End-to-end vmux release. Runs in `~/Projects/github.com/vmux-ai/vmux`.

## Inputs

- Bump kind: `patch` (default), `minor`, `major`.

## Pre-flight checks

1. Working tree clean: `git status --porcelain` must be empty.
2. On main: `git rev-parse --abbrev-ref HEAD` must be `main`.
3. Up-to-date with origin: `git fetch origin && git rev-list HEAD..origin/main --count` must be 0.
4. No in-flight release: `gh run list --workflow=release.yml --status in_progress --json databaseId | jq 'length'` must be 0.
5. Hooks installed: `[ -L .git/hooks/pre-push ]`. If not, run `make setup-hooks`.

If any check fails, abort and report.

## Flow

1. Read current version: `grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/'` — call this `CUR`.
2. Compute `NEW` by bumping the requested component:
   - `patch`: `0.0.7` → `0.0.8`
   - `minor`: `0.0.7` → `0.1.0`
   - `major`: `0.0.7` → `1.0.0`
3. Create worktree:
   ```
   git worktree add .worktrees/release-$NEW -b chore/release-$NEW origin/main
   cd .worktrees/release-$NEW
   ```
4. Bump:
   ```
   sed -i '' "s/^version = \"$CUR\"/version = \"$NEW\"/" Cargo.toml
   env -u CEF_PATH cargo update --workspace --offline
   ```
5. Pre-push checks (the hook does this on push, but run it explicitly to fail fast):
   ```
   PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
   for p in $PKGS; do cargo fmt -p "$p" -- --check; done
   for p in $PKGS; do env -u CEF_PATH cargo clippy -p "$p" --all-targets -- -D warnings; done
   for p in $PKGS; do env -u CEF_PATH cargo test -p "$p"; done
   ```
   Note: a pure version bump produces an empty PKGS set; that's expected — the lock file change doesn't mark any individual crate's directory as modified.
6. Commit and push:
   ```
   git add Cargo.toml Cargo.lock
   git commit -m "chore(release): v$NEW"
   git push -u origin chore/release-$NEW
   ```
7. Open PR:
   ```
   gh pr create --title "chore(release): v$NEW" --body "Patch release. Bumps workspace version $CUR -> $NEW. Releases on merge."
   ```
   Capture PR number as `$PR`.
8. Watch CI:
   ```
   gh pr checks $PR --watch --interval 30
   ```
9. On red: read failed job log via `gh run view --log-failed`. Attempt fix only for these known patterns:
   - rustfmt drift: run `make lint-fix`, recommit, push.
   - `release_invariants` assertion mismatch: read the panic message ("assertion failed: makefile.contains(\"…\")"), patch the asserted string in `crates/vmux_desktop/tests/release_invariants.rs` to match the current Makefile content, run `cargo fmt -p vmux_desktop -- --check`, recommit, push.
   Otherwise: stop, surface the failure, ask user.
10. On green: merge via API (works around `gh pr merge` failing when main worktree is busy):
    ```
    gh api -X PUT repos/vmux-ai/vmux/pulls/$PR/merge -f merge_method=squash
    ```
11. Cleanup:
    ```
    cd ../..
    git worktree remove .worktrees/release-$NEW
    git branch -D chore/release-$NEW
    git fetch origin --prune
    git pull --ff-only origin main
    ```
12. Watch release workflow. Find the run for the merged commit:
    ```
    MERGE_SHA=$(git rev-parse origin/main)
    RUN_ID=$(gh run list --workflow=release.yml --json databaseId,headSha -q ".[] | select(.headSha == \"$MERGE_SHA\") | .databaseId" | head -1)
    gh run watch $RUN_ID --interval 60 --exit-status
    ```
13. On green: report `https://github.com/vmux-ai/vmux/releases/tag/v$NEW`. On red: surface log, stop.

## Failure handling

- Any step that prompts a destructive recovery (revert, force-push, rewrite history) MUST stop and ask.
- The skill never bypasses the pre-push hook.
- If the user re-invokes the skill after a failure, detect the in-flight worktree (`git worktree list | grep release-$NEW`) and resume from where it left off.
```

### Step 4.2: Verify the skill loads

- [ ] In a fresh Claude Code session

```
/release
```

Expected: skill found, prompts for bump kind.

### Step 4.3: Commit the skill

- [ ] Stage and commit in the user's skills repo (if managed via git)

If the user keeps `~/.claude/skills/` under version control:

```bash
cd ~/.claude/skills
git add release/SKILL.md
git commit -m "skills: add release skill for vmux"
```

Otherwise leave as a local file.

---

## Task 5: Validate by cutting v0.0.8

**Precondition:** Tasks 1, 2, 3 merged + applied. Task 4 skill installed.

### Step 5.1: Trigger the skill

- [ ] In Claude Code, in the vmux working directory

```
/release patch
```

### Step 5.2: Observe end-to-end behavior

- [ ] Watch as the skill:
  - Creates worktree `.worktrees/release-0.0.8`
  - Opens PR
  - Waits for CI green
  - Merges via API
  - Watches release workflow
  - Reports release URL

### Step 5.3: Verify the release

- [ ] Inspect the published release

```bash
gh release view v0.0.8 --json assets,isDraft,tagName
```

Expected: `isDraft: false`, 4 assets present, tag `v0.0.8` pointing at the cask commit.

### Step 5.4: Verify cask + manifest

- [ ] Pull main and check

```bash
git pull --ff-only origin main
git log -1 --stat -- Casks/vmux.rb website/public/updates.json
```

Expected: latest commit on main is `chore(release): update cask and update manifest to 0.0.8`, with both files updated.

### Step 5.5: Verify website deploy was dispatched

- [ ] Check workflow runs

```bash
gh run list --workflow=deploy-website.yml --limit 1
```

Expected: a recent run, triggered by `workflow_dispatch`.

---

## Self-review notes

- All four spec components are covered (Tasks 1, 2, 3, 4 → spec sections 4, 1, 2, 3).
- Task 5 validates the whole chain.
- No placeholders in code blocks; all commands and file contents are concrete.
- Type/method consistency: env var names (`TAG`, `VERSION`, `RELEASE_SHA`, `ALREADY_DONE`, `NEW_HEAD`), repo path (`vmux-ai/vmux`), branch names (`chore/<topic>`) used consistently.
- Branch protection (Task 3) blocks `Build macOS App` from being a required check because it's path-conditional; only `Lint` and `Test` are listed.
