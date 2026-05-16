---
name: vmux-release
description: Use when releasing Vmux, bumping Vmux versions, testing Vmux release automation, or checking GitHub Actions release/tag workflows for vmux-ai/vmux.
---

# Vmux Release

Release is driven by a reviewed version bump. Do not manually create release tags.

## Flow

1. Start from current `main`.

```sh
git switch main
git pull --rebase origin main
```

2. Create release branch.

```sh
VERSION=0.0.2
git switch -c "release/v$VERSION"
```

3. Bump root `Cargo.toml`.

```sh
perl -0pi -e "s/version = \"[0-9]+\\.[0-9]+\\.[0-9]+[^\\\"]*\"/version = \"$VERSION\"/" Cargo.toml
cargo update -w
```

4. Verify before commit.

```sh
make lint
make test
```

5. Commit, push, open PR.

```sh
git add Cargo.toml Cargo.lock
git commit -m "chore(release): v$VERSION"
git push -u origin "release/v$VERSION"

gh pr create \
  -B main \
  -H "release/v$VERSION" \
  -t "chore(release): v$VERSION" \
  -b "Bump workspace version to $VERSION."
```

6. Merge after PR CI passes.

```sh
gh pr checks --watch
gh pr merge --squash --delete-branch
```

7. Watch automation.

```sh
gh run list --workflow CI --branch main --limit 1
gh run list --workflow "Tag Release" --limit 1
gh run list --workflow Release --limit 1
git ls-remote --tags origin "v$VERSION"
```

Expected sequence:

```txt
main CI passes
Tag Release creates v$VERSION
Release runs from v$VERSION
```

## Rules

- Version bump must happen in root `Cargo.toml` under `[workspace.package]`.
- `Cargo.lock` may or may not change; include it only if changed.
- If version did not change from previous commit, `Tag Release` skips.
- If tag already exists, `Tag Release` fails. Pick a new version.
- Never run `git tag` or push tags manually for normal releases.
- If `make lint` or `make test` fails, fix before PR or merge.

