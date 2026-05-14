# Hands-Free Release — Design

Date: 2026-05-14
Status: Approved
Target: v0.0.8

## Goal

Make patch releases hands-free, error-free, and effort-free. Single command (`/release patch`) takes the project from current state to a published GitHub Release + updated Homebrew cask + deployed website manifest, with no human interaction in the happy path.

## Motivation

The v0.0.7 release surfaced four distinct failure modes that required manual recovery:

1. **Direct push to main bypassed CI**, breaking `release_invariants` test that asserted Makefile content.
2. **Race condition**: a workflow-file PR (`#34`) merged while v0.0.7 release was building. Tag push got rejected by GitHub's `workflows`-permission rule because the tag commit's `ci.yml` differed from main's `ci.yml`.
3. **Re-run drift**: rebuilt DMG had a different SHA than the first attempt (non-deterministic build), causing rebase conflict on the cask file.
4. **Test-selection blind spot**: AGENTS.md's changed-crate loop only tests crates whose own files changed. `release_invariants.rs` `include_str!`s the workspace Makefile from outside its crate, so changes to Makefile didn't trigger its test.

Each failure was recoverable but required diagnosing logs, manually pushing tags, reverting commits, and re-running workflows. Multiply by every release.

## Non-Goals

- Multi-platform releases (Linux/Windows). Mac-only stays.
- Reproducible builds (DMG SHA stability). Solved structurally instead — see release.yml refactor.
- Auto-recovery from arbitrary CI failures. The skill stops on unknown failures and surfaces them.

## Components

### 1. release.yml — transactional refactor

Current flow: build → commit cask to main → push tag → create release. Fails halfway leave main with stale cask referencing assets that were never published.

New flow:

```
detect-version (unchanged)
  ↓
release-macos:
  1. Build, sign, notarize → DMG + tarballs locally
  2. Re-run guard: gh release view $TAG → if exists, published, and has all assets → exit 0
  3. Create or upload to draft release:
       gh release view $TAG --json isDraft → exists?
         no:  gh release create $TAG --draft --target $RELEASE_SHA --generate-notes <assets>
         yes: gh release upload $TAG --clobber <assets>
  4. Compute SHAs from local artifacts; update Casks/vmux.rb + website/public/updates.json
  5. Commit cask + manifest; git pull --rebase origin main; git push origin HEAD:main
  6. Re-target tag to current main HEAD:
       NEW_HEAD=$(git rev-parse HEAD)
       gh api -X PATCH repos/$OWNER/$REPO/git/refs/tags/$TAG \
         -f sha=$NEW_HEAD -F force=true
  7. Publish release: gh release edit $TAG --draft=false
  8. Dispatch website deploy: gh workflow run deploy-website.yml --ref main
```

**Idempotency contract:** every step is safe to re-run. Until step 7, the release is a draft and invisible to users. Cask commit on main is the single source of truth for what shipped — second attempt rebases cleanly because cask values match what's in the draft release (it was uploaded in step 3 of the same attempt).

**Why tag re-targeting works:** GitHub's `workflows`-permission rule blocks tag pushes whose target commit's `.github/workflows/*` content differs from the default branch. By re-targeting the tag to current main HEAD (after the cask commit), the tag's tree matches main's tree, so no divergence, no rejection.

The `gh api -X PATCH .../git/refs/tags/$TAG` call uses the GitHub API (not git push), which uses the `contents: write` token permission and is not subject to the git pre-receive hook that enforces the `workflows` rule.

### 2. Branch protection on main

GitHub Settings → Branches → main:

- ✅ Require a pull request before merging (no required reviews — solo project)
- ✅ Require status checks to pass before merging:
  - `Lint` (always runs)
  - `Test` (always runs)
  - `Build macOS App` is **not** required because it's path-conditional; required-status-check rules treat skipped jobs as missing
- ✅ Require linear history (squash or rebase only)
- ✅ Restrict who can push to matching branches → bypass list: `github-actions[bot]` only

The bot bypass is necessary because the release workflow's cask commit step pushes directly to main (step 5 of the new release flow). Without bypass, the workflow would need to open a PR for cask updates and auto-merge it — extra CI run, extra latency, and the cask PR would also need bot bypass to merge.

### 3. /release skill

Slash command: `/release patch | minor | major`

Skill location: `~/.claude/skills/release/SKILL.md`

Flow:

1. Read current version from workspace `Cargo.toml`. Compute next version by bumping the requested component.
2. Verify clean state: no uncommitted changes; main is up-to-date with origin.
3. Verify no in-flight `release.yml` runs: `gh run list --workflow=release.yml --status in_progress`. Abort if any.
4. Create worktree: `git worktree add .worktrees/release-X.Y.Z -b chore/release-X.Y.Z origin/main`.
5. Bump `Cargo.toml` workspace version + `cargo update --workspace --offline`.
6. Run pre-push checks (see Component 4): fmt + clippy + test on changed crates AND crates whose tests `include_str!` from changed paths.
7. Commit `chore(release): vX.Y.Z`, push branch.
8. `gh pr create` with title `chore(release): vX.Y.Z` and a body listing what changed.
9. Poll `gh pr checks $PR --watch` until done.
10. On red: read failed job log, attempt fix only for known patterns:
    - rustfmt drift → run `make lint-fix` in worktree, recommit, push
    - `release_invariants` assertion mismatch → patch the asserted string to match new Makefile, recommit, push
    Otherwise: surface error, stop, ask user.
11. On green: merge via API: `gh api -X PUT repos/.../pulls/$PR/merge -f merge_method=squash`.
12. Pull main locally, remove worktree + branch.
13. Watch `release.yml` workflow for the merged commit until done.
14. On green: report Release URL. On red: surface error log, stop.

The skill is rigid — it follows the script, doesn't improvise. Unknown failures stop the flow.

### 4. Pre-push test hook

Replace AGENTS.md's inline changed-crate snippet with a script that also follows `include_str!` references.

Script: `scripts/changed-crates.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail
BASE="${BASE:-origin/main}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# Crates whose own files changed
crates_by_dir() {
  cargo metadata --no-deps --format-version 1 \
    | jq -r '.packages[]
        | select(.manifest_path | test("patches") | not)
        | "\(.name)\t\(.manifest_path | sub("/Cargo\\.toml$"; ""))"' \
    | while IFS=$'\t' read -r name dir; do
        rel="${dir#"$ROOT"/}"
        [ -z "$rel" ] && rel="."
        if ! git diff --quiet "$BASE" -- "$rel"; then
          echo "$name"
        fi
      done
}

# Crates that include_str! from a changed path
# include_str!("...") is resolved by rustc relative to the source file's
# directory, so we must resolve each match against its file's dirname.
crates_by_includes() {
  changed=$(git diff --name-only "$BASE")
  cargo metadata --no-deps --format-version 1 \
    | jq -r '.packages[]
        | select(.manifest_path | test("patches") | not)
        | "\(.name)\t\(.manifest_path | sub("/Cargo\\.toml$"; ""))"' \
    | while IFS=$'\t' read -r name dir; do
        grep -rn 'include_str!("[^"]*")' "$dir" 2>/dev/null \
          | while IFS=':' read -r file _lineno match; do
              inc=$(printf '%s' "$match" | sed -E 's/.*include_str!\("([^"]*)"\).*/\1/')
              file_dir=$(dirname "$file")
              abs=$(cd "$file_dir" && realpath -m "$inc" 2>/dev/null) || continue
              rel="${abs#"$ROOT"/}"
              if printf '%s\n' "$changed" | grep -qx "$rel"; then
                echo "$name"
              fi
            done
      done | sort -u
}

(crates_by_dir; crates_by_includes) | sort -u
```

Wired in two places:

- `.git/hooks/pre-push` — installed by `make setup-hooks` (new target). Runs `cargo test -p $crate` for each crate the script outputs. Blocks push on failure.
- AGENTS.md updated to point at `scripts/changed-crates.sh` instead of the inline snippet. The rule is enforceable instead of advisory.

The hook is local and skippable with `--no-verify`. The skill respects the hook (does not pass `--no-verify`).

## Risks and Tradeoffs

- **Tag re-targeting drops reproducibility for the version-bump commit.** Today the v0.0.7 tag points to `feff9c4`, the version-bump commit. Under the new flow the tag will point to the cask commit (one commit later). The version-bump is a parent of the tagged commit, so `git log v0.0.8` still includes it. Acceptable.
- **Bot bypass on branch protection** lets the release workflow push to main without CI. A compromised workflow file could push arbitrary content. Mitigated by requiring PR review for any change to `.github/workflows/**` (could be enforced via CODEOWNERS later; out of scope for v0.0.8).
- **Pre-push hook is local-only.** New contributors need `make setup-hooks`. Low risk for solo project; the script also runs in the `/release` skill so it's caught there too.
- **`include_str!` resolution.** The script resolves paths relative to each matching source file's directory (matching rustc's behavior). Edge cases: macros generating `include_str!` calls, conditionally-compiled modules — both are uncommon and outside scope. The existing `release_invariants.rs` case is the primary target.

## Implementation Order

1. Pre-push hook script + AGENTS.md update (lowest risk, immediately useful).
2. release.yml refactor (highest impact for next release).
3. Branch protection (one-time GitHub UI change; documented in this spec).
4. /release skill (depends on 2 being live).

Each step ships as its own PR.
