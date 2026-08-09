use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use similar::{ChangeTag, TextDiff};

use crate::event::*;
use crate::parse;

#[derive(Debug, Clone)]
pub struct GitError(pub String);

/// Repository-local `GIT_*` variables used when `git rev-parse --local-env-vars`
/// cannot be queried. Mirrors Git's own list (git 2.54); the live query in
/// [`local_env_vars`] supersedes it whenever git is runnable.
const FALLBACK_LOCAL_ENV_VARS: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_CONFIG",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_DIR",
    "GIT_GRAFT_FILE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_REPLACE_REF_BASE",
    "GIT_SHALLOW_FILE",
    "GIT_WORK_TREE",
];

/// Every repository-local `GIT_*` variable Git recognizes, queried once from the
/// authoritative `git rev-parse --local-env-vars` and cached (falling back to
/// [`FALLBACK_LOCAL_ENV_VARS`] if git cannot be run).
///
/// The runner always targets an explicit repository via [`Command::current_dir`],
/// so these ambient variables — which a parent process such as a `git push`
/// pre-push hook exports — must be stripped; otherwise `GIT_DIR`, `GIT_INDEX_FILE`,
/// `GIT_OBJECT_DIRECTORY`, `GIT_CONFIG` and friends override the explicit target and
/// the call silently reads or writes the wrong repository. Listing the names is a
/// static print, so it is safe to run under any ambient environment.
fn local_env_vars() -> &'static [String] {
    static VARS: OnceLock<Vec<String>> = OnceLock::new();
    VARS.get_or_init(|| {
        Command::new("git")
            .args(["rev-parse", "--local-env-vars"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .filter(|vars| !vars.is_empty())
            .unwrap_or_else(|| {
                FALLBACK_LOCAL_ENV_VARS
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            })
    })
}

/// Build a `git` [`Command`] rooted at `root` with a scrubbed environment.
///
/// Clears every repository-local `GIT_*` variable (see [`local_env_vars`]) so an
/// ambient environment cannot redirect the call away from its `current_dir` target.
fn git_command(root: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(root).env("GIT_TERMINAL_PROMPT", "0");
    for var in local_env_vars() {
        cmd.env_remove(var);
    }
    cmd
}

pub(crate) fn git(root: &Path, args: &[&str]) -> Result<(String, String, bool), GitError> {
    let out = git_command(root)
        .args(args)
        .output()
        .map_err(|e| GitError(format!("failed to run git: {e}")))?;
    Ok((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    ))
}

pub(crate) fn git_read(root: &Path, args: &[&str]) -> Result<(String, String, bool), GitError> {
    let out = git_command(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .output()
        .map_err(|e| GitError(format!("failed to run git: {e}")))?;
    Ok((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    ))
}

pub(crate) fn git_err(stdout: &str, stderr: &str) -> GitError {
    let s = stderr.trim();
    GitError(if s.is_empty() {
        stdout.trim().to_string()
    } else {
        s.to_string()
    })
}

fn start_dir(file: &Path) -> PathBuf {
    if file.is_dir() {
        file.to_path_buf()
    } else {
        file.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

pub fn has_repository(file: &Path) -> bool {
    start_dir(file)
        .ancestors()
        .any(|directory| directory.join(".git").exists())
}

pub(crate) fn non_repository_status() -> GitStatusEvent {
    GitStatusEvent {
        branch: String::new(),
        ahead: 0,
        behind: 0,
        has_upstream: false,
        file_status: FileStatus::Clean,
        staged_count: 0,
        repo_root: String::new(),
    }
}

pub fn repo_root(file: &Path) -> Result<PathBuf, GitError> {
    let (stdout, stderr, ok) = git(&start_dir(file), &["rev-parse", "--show-toplevel"])?;
    if !ok {
        return Err(GitError(stderr.trim().to_string()));
    }
    Ok(PathBuf::from(stdout.trim()))
}

fn canon(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf())
                .join(name),
            _ => path.to_path_buf(),
        })
}

fn rel(root: &Path, file: &Path) -> String {
    let root = canon(root);
    let file = canon(file);
    file.strip_prefix(&root)
        .unwrap_or(&file)
        .to_string_lossy()
        .into_owned()
}

pub fn status(file: &Path) -> Result<GitStatusEvent, GitError> {
    let root = repo_root(file)?;
    statuses(&root, &[file.to_path_buf()])?
        .pop()
        .ok_or_else(|| GitError("missing git status result".into()))
}

pub fn file_statuses(
    root: &Path,
) -> Result<std::collections::HashMap<String, FileStatus>, GitError> {
    let (stdout, stderr, ok) = git_read(
        root,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
        ],
    )?;
    if !ok {
        return Err(GitError(stderr.trim().to_string()));
    }
    Ok(parse::parse_porcelain_v2_statuses(&stdout).into_file_statuses())
}

pub(crate) fn statuses(root: &Path, files: &[PathBuf]) -> Result<Vec<GitStatusEvent>, GitError> {
    let (stdout, stderr, ok) = git_read(
        root,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
        ],
    )?;
    if !ok {
        return Err(GitError(stderr.trim().to_string()));
    }
    let repo_root = root.to_string_lossy().into_owned();
    let parsed = parse::parse_porcelain_v2_statuses(&stdout);
    Ok(files
        .iter()
        .map(|file| {
            let target = rel(root, file);
            GitStatusEvent {
                branch: parsed.branch.clone(),
                ahead: parsed.ahead,
                behind: parsed.behind,
                has_upstream: parsed.has_upstream,
                file_status: parsed.file_status(&target),
                staged_count: parsed.staged_count,
                repo_root: repo_root.clone(),
            }
        })
        .collect())
}

/// Repo root plus the set of repo-relative paths `git status --porcelain=v2`
/// reports as changed (modified/staged/untracked/renamed/deleted/conflicted).
pub fn dirty_set(file: &Path) -> Result<(PathBuf, std::collections::HashSet<String>), GitError> {
    let root = repo_root(file)?;
    let (stdout, stderr, ok) = git_read(
        &root,
        &["status", "--porcelain=v2", "--untracked-files=all"],
    )?;
    if !ok {
        return Err(GitError(stderr.trim().to_string()));
    }
    Ok((root, parse::changed_paths(&stdout)))
}

fn diff_text(root: &Path, target: &str, cached: bool, ctx: u32) -> Result<String, GitError> {
    let uarg = format!("--unified={ctx}");
    let mut args: Vec<&str> = vec!["diff"];
    if cached {
        args.push("--cached");
    }
    args.push(&uarg);
    args.push("--");
    args.push(target);
    let (out, stderr, ok) = git(root, &args)?;
    if ok {
        Ok(out)
    } else {
        Err(GitError(stderr.trim().to_string()))
    }
}

fn tag_hunk(line: &DiffLine, ranges: &[parse::HunkRange]) -> Option<u32> {
    match line.kind {
        DiffKind::Add => {
            let n = line.new_no?;
            ranges
                .iter()
                .position(|r| n >= r.new_start && n < r.new_start + r.new_count)
                .map(|i| i as u32)
        }
        DiffKind::Remove => {
            let o = line.old_no?;
            ranges
                .iter()
                .position(|r| o >= r.old_start && o < r.old_start + r.old_count)
                .map(|i| i as u32)
        }
        _ => None,
    }
}

fn staged_lineset(root: &Path, target: &str) -> HashSet<u32> {
    diff_text(root, target, true, 0)
        .map(|t| {
            parse::hunk_ranges(&t)
                .iter()
                .flat_map(|r| r.new_start..r.new_start + r.new_count)
                .collect()
        })
        .unwrap_or_default()
}

fn staged_only_lines(
    file: &Path,
    root: &Path,
    target: &str,
    staged: &HashSet<u32>,
) -> Result<Vec<DiffLine>, GitError> {
    if diff_text(root, target, true, 100_000)?.trim().is_empty() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(file).unwrap_or_default();
    let spans = crate::highlight::highlight_file(&content, file);
    let lines = content
        .lines()
        .enumerate()
        .map(|(i, _)| {
            let n = i as u32 + 1;
            DiffLine {
                kind: if staged.contains(&n) {
                    DiffKind::Staged
                } else {
                    DiffKind::Context
                },
                old_no: Some(n),
                new_no: Some(n),
                hunk: None,
                spans: spans.get(i).cloned().unwrap_or_default(),
            }
        })
        .collect();
    Ok(lines)
}

fn index_text(root: &Path, target: &str) -> Result<String, GitError> {
    let spec = format!(":{target}");
    let (out, _, ok) = git(root, &["show", &spec])?;
    if ok { Ok(out) } else { Ok(String::new()) }
}

pub fn diff_lines_with_content(file: &Path, content: &str) -> Result<Vec<DiffLine>, GitError> {
    let root = repo_root(file)?;
    let target = rel(&root, file);
    let baseline = index_text(&root, &target)?;
    let staged = staged_lineset(&root, &target);
    let new_spans = crate::highlight::highlight_file(content, file);
    let mut old_no = 1u32;
    let mut new_no = 1u32;
    let mut lines = Vec::new();

    for change in TextDiff::from_lines(baseline.as_str(), content).iter_all_changes() {
        let text = change.value().trim_end_matches(['\n', '\r']);
        match change.tag() {
            ChangeTag::Equal => {
                lines.push(DiffLine {
                    kind: if staged.contains(&old_no) {
                        DiffKind::Staged
                    } else {
                        DiffKind::Context
                    },
                    old_no: Some(old_no),
                    new_no: Some(new_no),
                    hunk: None,
                    spans: new_spans
                        .get(new_no.saturating_sub(1) as usize)
                        .cloned()
                        .unwrap_or_else(|| crate::highlight::highlight_line(text, file)),
                });
                old_no += 1;
                new_no += 1;
            }
            ChangeTag::Delete => {
                lines.push(DiffLine {
                    kind: DiffKind::Remove,
                    old_no: Some(old_no),
                    new_no: None,
                    hunk: None,
                    spans: crate::highlight::highlight_line(text, file),
                });
                old_no += 1;
            }
            ChangeTag::Insert => {
                lines.push(DiffLine {
                    kind: DiffKind::Add,
                    old_no: None,
                    new_no: Some(new_no),
                    hunk: None,
                    spans: new_spans
                        .get(new_no.saturating_sub(1) as usize)
                        .cloned()
                        .unwrap_or_else(|| crate::highlight::highlight_line(text, file)),
                });
                new_no += 1;
            }
        }
    }
    Ok(lines)
}

pub fn diff_lines(file: &Path) -> Result<Vec<DiffLine>, GitError> {
    let root = repo_root(file)?;
    let target = rel(&root, file);
    let staged = staged_lineset(&root, &target);

    let unstaged = diff_text(&root, &target, false, 100_000)?;
    if unstaged.trim().is_empty() {
        return staged_only_lines(file, &root, &target, &staged);
    }
    let ranges = parse::hunk_ranges(&diff_text(&root, &target, false, 0)?);

    let new_spans = std::fs::read_to_string(file)
        .map(|c| crate::highlight::highlight_file(&c, file))
        .unwrap_or_default();

    let lines = parse::parse_unified_diff(&unstaged)
        .into_iter()
        .filter(|l| !matches!(l.kind, DiffKind::Hunk))
        .map(|mut l| {
            l.hunk = tag_hunk(&l, &ranges);
            let text = l.spans.first().map(|s| s.text.clone()).unwrap_or_default();
            l.spans = match l.kind {
                DiffKind::Add | DiffKind::Context => l
                    .new_no
                    .and_then(|n| new_spans.get(n.saturating_sub(1) as usize))
                    .cloned()
                    .unwrap_or_else(|| crate::highlight::highlight_line(&text, file)),
                _ => crate::highlight::highlight_line(&text, file),
            };
            if matches!(l.kind, DiffKind::Context) && l.old_no.is_some_and(|o| staged.contains(&o))
            {
                l.kind = DiffKind::Staged;
            }
            l
        })
        .collect();
    Ok(lines)
}

fn git_apply(root: &Path, patch: &str, reverse: bool) -> Result<(), GitError> {
    use std::io::Write;
    use std::process::Stdio;
    let mut args: Vec<&str> = vec!["apply"];
    if reverse {
        args.push("-R");
    } else {
        args.push("--cached");
    }
    args.push("--unidiff-zero");
    let mut child = git_command(root)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| GitError(format!("failed to run git apply: {e}")))?;
    child
        .stdin
        .take()
        .ok_or_else(|| GitError("git apply: no stdin".into()))?
        .write_all(patch.as_bytes())
        .map_err(|e| GitError(format!("git apply write: {e}")))?;
    let out = child
        .wait_with_output()
        .map_err(|e| GitError(format!("git apply wait: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(git_err(
            &String::from_utf8_lossy(&out.stdout),
            &String::from_utf8_lossy(&out.stderr),
        ))
    }
}

pub fn apply_hunk(file: &Path, index: u32, accept: bool) -> Result<(), GitError> {
    let root = repo_root(file)?;
    let target = rel(&root, file);
    let diff = diff_text(&root, &target, false, 0)?;
    if diff.trim().is_empty() {
        return Err(GitError("no unstaged changes for this file".into()));
    }
    let (header, hunks) = parse::hunk_patches(&diff);
    let body = hunks
        .get(index as usize)
        .ok_or_else(|| GitError("hunk index out of range".into()))?;
    let patch = format!("{header}{body}");
    git_apply(&root, &patch, !accept)
}

fn simple(file: &Path, verb: &[&str]) -> Result<(), GitError> {
    let root = repo_root(file)?;
    let target = rel(&root, file);
    let mut args: Vec<&str> = verb.to_vec();
    args.push(&target);
    let (stdout, stderr, ok) = git(&root, &args)?;
    if ok {
        Ok(())
    } else {
        Err(git_err(&stdout, &stderr))
    }
}

pub fn stage(file: &Path) -> Result<(), GitError> {
    simple(file, &["add", "--"])
}

pub fn unstage(file: &Path) -> Result<(), GitError> {
    simple(file, &["restore", "--staged", "--"])
}

pub fn discard(file: &Path) -> Result<(), GitError> {
    simple(file, &["restore", "--"])
}

pub fn commit(file: &Path, message: &str) -> Result<(), GitError> {
    let root = repo_root(file)?;
    let (stdout, stderr, ok) = git(&root, &["commit", "-m", message])?;
    if ok {
        Ok(())
    } else {
        Err(git_err(&stdout, &stderr))
    }
}

pub fn push(file: &Path) -> Result<(), GitError> {
    let root = repo_root(file)?;
    let (stdout, stderr, ok) = git(&root, &["push"])?;
    if ok {
        Ok(())
    } else {
        Err(git_err(&stdout, &stderr))
    }
}

#[cfg(test)]
#[path = "runner.test_repo.test.rs"]
pub(crate) mod test_repo;
#[cfg(test)]
#[path = "runner.test.rs"]
mod tests;
