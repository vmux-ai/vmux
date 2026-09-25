use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use similar::{ChangeTag, TextDiff};

use crate::event::*;
use crate::host::parse;

#[derive(Debug, Clone)]
pub struct GitError(pub String);

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
    let (stdout, stderr, ok) = git_read_bytes(root, args)?;
    Ok((String::from_utf8_lossy(&stdout).into_owned(), stderr, ok))
}

fn git_read_bytes(root: &Path, args: &[&str]) -> Result<(Vec<u8>, String, bool), GitError> {
    let out = git_command(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .output()
        .map_err(|e| GitError(format!("failed to run git: {e}")))?;
    Ok((
        out.stdout,
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

pub(crate) fn non_repository_status(path: &Path) -> GitFileStatus {
    GitFileStatus {
        path: path.to_string_lossy().into_owned(),
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
    vmux_path::PathIdentity::resolve(path).into_path_buf()
}

fn rel(root: &Path, file: &Path) -> PathBuf {
    let root = canon(root);
    let file = canon(file);
    file.strip_prefix(&root).unwrap_or(&file).to_path_buf()
}

#[cfg(unix)]
fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;

    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;

    PathBuf::from(OsString::from_vec(bytes.to_vec()))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

pub(crate) struct RequestPath<'a> {
    display: &'a str,
    bytes: &'a [u8],
}

impl<'a> RequestPath<'a> {
    pub(crate) fn new(display: &'a str, bytes: &'a [u8]) -> Self {
        Self { display, bytes }
    }

    pub(crate) fn resolve(&self, repo_root: &Path) -> PathBuf {
        if self.bytes.is_empty() {
            return PathBuf::from(self.display);
        }
        repo_root.join(path_from_bytes(self.bytes))
    }
}

pub fn status(file: &Path) -> Result<GitFileStatus, GitError> {
    let root = repo_root(file)?;
    status_at(&root, file)
}

pub fn status_at(root: &Path, file: &Path) -> Result<GitFileStatus, GitError> {
    statuses(root, &[file.to_path_buf()])?
        .pop()
        .ok_or_else(|| GitError("missing git status result".into()))
}

pub fn file_statuses(
    root: &Path,
) -> Result<std::collections::HashMap<String, FileStatus>, GitError> {
    let (stdout, stderr, ok) = git_read_bytes(
        root,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=all",
        ],
    )?;
    if !ok {
        return Err(GitError(stderr.trim().to_string()));
    }
    Ok(parse::parse_porcelain_v2_statuses(&stdout).into_file_statuses())
}

impl GitCommitEntry {
    fn recent(root: &Path) -> Result<Vec<Self>, GitError> {
        let (_, _, has_head) = git_read(root, &["rev-parse", "--verify", "HEAD"])?;
        if !has_head {
            return Ok(Vec::new());
        }
        let (stdout, stderr, ok) = git_read(
            root,
            &[
                "log",
                "-50",
                "--date=short",
                "--format=%H%x00%h%x00%an%x00%ad%x00%s",
            ],
        )?;
        if !ok {
            return Err(git_err(&stdout, &stderr));
        }
        let mut commits = Vec::new();
        for line in stdout.lines() {
            let fields = line.split('\0').collect::<Vec<_>>();
            if fields.len() != 5 {
                continue;
            }
            commits.push(Self {
                sha: fields[0].to_string(),
                short_sha: fields[1].to_string(),
                author: fields[2].to_string(),
                date: fields[3].to_string(),
                summary: fields[4].to_string(),
                body: String::new(),
                references: String::new(),
            });
        }
        Ok(commits)
    }

    pub(crate) fn for_reference(root: &Path, reference: &str) -> Result<Vec<Self>, GitError> {
        let (stdout, stderr, ok) = git_read(
            root,
            &[
                "log",
                "-50",
                "--date=relative",
                "--format=%H%x1f%h%x1f%an%x1f%ar%x1f%s%x1f%b%x1f%D%x1e",
                reference,
                "--",
            ],
        )?;
        if !ok {
            return Err(git_err(&stdout, &stderr));
        }
        let mut commits = Vec::new();
        for record in stdout.split('\x1e') {
            let record = record.trim_matches(['\n', '\r']);
            if record.is_empty() {
                continue;
            }
            let fields = record.splitn(7, '\x1f').collect::<Vec<_>>();
            if fields.len() != 7 {
                continue;
            }
            commits.push(Self {
                sha: fields[0].to_string(),
                short_sha: fields[1].to_string(),
                author: fields[2].to_string(),
                date: fields[3].to_string(),
                summary: fields[4].to_string(),
                body: fields[5].trim().to_string(),
                references: fields[6].trim().to_string(),
            });
        }
        Ok(commits)
    }
}

impl GitBranchEntry {
    fn local(root: &Path, current: &str) -> Result<Vec<Self>, GitError> {
        let registrations = crate::host::worktree::worktree_registrations(root)?;
        let (_, _, has_head) = git_read(root, &["rev-parse", "--verify", "HEAD"])?;
        let format = if has_head {
            "--format=%(refname:short)%00%(HEAD)%00%(upstream:short)%00%(objectname:short)%00%(ahead-behind:HEAD)"
        } else {
            "--format=%(refname:short)%00%(HEAD)%00%(upstream:short)%00%(objectname:short)%00"
        };
        let (stdout, stderr, ok) = git_read(
            root,
            &[
                "for-each-ref",
                "--sort=-committerdate",
                format,
                "refs/heads",
            ],
        )?;
        if !ok {
            return Err(git_err(&stdout, &stderr));
        }
        let mut branches = Vec::new();
        for line in stdout.lines() {
            let fields = line.split('\0').collect::<Vec<_>>();
            if fields.len() != 5 {
                continue;
            }
            let (ahead, behind) = Self::relation(fields[4]);
            branches.push(Self {
                name: fields[0].to_string(),
                current: fields[1] == "*",
                upstream: fields[2].to_string(),
                checkout: registrations
                    .iter()
                    .find(|registration| registration.branch.as_deref() == Some(fields[0]))
                    .map(|registration| registration.path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                short_sha: fields[3].to_string(),
                ahead,
                behind,
            });
        }
        if branches.is_empty() && !current.is_empty() && current != "(detached)" {
            branches.push(Self {
                name: current.to_string(),
                current: true,
                upstream: String::new(),
                checkout: root.to_string_lossy().into_owned(),
                short_sha: String::new(),
                ahead: 0,
                behind: 0,
            });
        }
        Ok(branches)
    }

    fn remote(root: &Path) -> Result<Vec<Self>, GitError> {
        let (_, _, has_head) = git_read(root, &["rev-parse", "--verify", "HEAD"])?;
        let format = if has_head {
            "--format=%(refname:short)%00%(symref)%00%(objectname:short)%00%(ahead-behind:HEAD)"
        } else {
            "--format=%(refname:short)%00%(symref)%00%(objectname:short)%00"
        };
        let (stdout, stderr, ok) = git_read(
            root,
            &[
                "for-each-ref",
                "--sort=-committerdate",
                format,
                "refs/remotes",
            ],
        )?;
        if !ok {
            return Err(git_err(&stdout, &stderr));
        }
        let mut branches = Vec::new();
        for line in stdout.lines() {
            let fields = line.split('\0').collect::<Vec<_>>();
            if fields.len() != 4 || !fields[1].is_empty() {
                continue;
            }
            let (ahead, behind) = Self::relation(fields[3]);
            branches.push(Self {
                name: fields[0].to_string(),
                current: false,
                upstream: String::new(),
                checkout: String::new(),
                short_sha: fields[2].to_string(),
                ahead,
                behind,
            });
        }
        Ok(branches)
    }

    fn relation(value: &str) -> (u32, u32) {
        let mut counts = value.split_whitespace();
        let ahead = counts
            .next()
            .and_then(|count| count.parse().ok())
            .unwrap_or(0);
        let behind = counts
            .next()
            .and_then(|count| count.parse().ok())
            .unwrap_or(0);
        (ahead, behind)
    }
}

impl GitTagEntry {
    fn list(root: &Path) -> Result<Vec<Self>, GitError> {
        let (stdout, stderr, ok) = git_read(
            root,
            &[
                "for-each-ref",
                "--sort=-creatordate",
                "--format=%(refname:short)%00%(objectname:short)%00%(creatordate:relative)%00%(subject)",
                "refs/tags",
            ],
        )?;
        if !ok {
            return Err(git_err(&stdout, &stderr));
        }
        let mut tags = Vec::new();
        for line in stdout.lines() {
            let fields = line.splitn(4, '\0').collect::<Vec<_>>();
            if fields.len() != 4 {
                continue;
            }
            tags.push(Self {
                name: fields[0].to_string(),
                short_sha: fields[1].to_string(),
                date: fields[2].to_string(),
                message: fields[3].to_string(),
            });
        }
        Ok(tags)
    }
}

impl GitStashEntry {
    fn list(root: &Path) -> Result<Vec<Self>, GitError> {
        let (stdout, stderr, ok) = git_read(root, &["stash", "list", "--format=%gd%x00%gs"])?;
        if !ok {
            return Err(git_err(&stdout, &stderr));
        }
        let mut entries = Vec::new();
        for line in stdout.lines() {
            let Some((reference, message)) = line.split_once('\0') else {
                continue;
            };
            let index = reference
                .strip_prefix("stash@{")
                .and_then(|value| value.strip_suffix('}'))
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(entries.len() as u32);
            entries.push(Self {
                index,
                reference: reference.to_string(),
                message: message.to_string(),
            });
        }
        Ok(entries)
    }
}

impl GitOperation {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Amend => "amend",
            Self::CheckoutCommit { .. } => "checkout commit",
            Self::CherryPick { .. } => "cherry-pick",
            Self::CreateBranch { .. } => "new branch",
            Self::DeleteBranch { .. } => "delete branch",
            Self::FastForward { .. } => "fast-forward",
            Self::Merge { .. } => "merge",
            Self::Rebase { .. } => "rebase",
            Self::Revert { .. } => "revert",
            Self::StashDrop { .. } => "stash drop",
            Self::StashPop { .. } => "stash pop",
            Self::StashPush => "stash",
        }
    }

    pub(crate) fn run(&self, root: &Path) -> Result<String, GitError> {
        if let Self::CreateBranch { branch, .. } = self {
            crate::host::worktree::validate_branch_name(root, branch)?;
        }
        let args = match self {
            Self::Amend => vec!["commit", "--amend", "--no-edit"],
            Self::CheckoutCommit { commit } => vec!["switch", "--detach", commit],
            Self::CherryPick { commit } => vec!["cherry-pick", commit],
            Self::CreateBranch {
                branch,
                start_point,
            } => vec!["branch", "--", branch, start_point],
            Self::DeleteBranch { branch } => vec!["branch", "-d", "--", branch],
            Self::FastForward { branch } => vec!["merge", "--ff-only", branch],
            Self::Merge { branch } => vec!["merge", "--no-edit", branch],
            Self::Rebase { branch } => vec!["rebase", branch],
            Self::Revert { commit } => vec!["revert", "--no-edit", commit],
            Self::StashDrop { reference } => vec!["stash", "drop", reference],
            Self::StashPop { reference } => vec!["stash", "pop", reference],
            Self::StashPush => vec!["stash", "push", "--include-untracked"],
        };
        let (stdout, stderr, ok) = git(root, &args)?;
        if !ok {
            return Err(git_err(&stdout, &stderr));
        }
        let message = if stdout.trim().is_empty() {
            stderr.trim()
        } else {
            stdout.trim()
        };
        Ok(if message.is_empty() {
            "ok".to_string()
        } else {
            message.to_string()
        })
    }
}

impl GitRepositorySnapshot {
    pub fn load(path: &Path) -> Result<Self, GitError> {
        let requested_path = path.to_string_lossy().into_owned();
        let repo_root = repo_root(path)?;
        let (stdout, stderr, ok) = git_read_bytes(
            &repo_root,
            &[
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=all",
            ],
        )?;
        if !ok {
            return Err(git_err(&String::from_utf8_lossy(&stdout), &stderr));
        }
        let parsed = parse::parse_porcelain_v2_statuses(&stdout);
        let branch = parsed.branch.clone();
        let upstream = parsed.upstream.clone();
        let ahead = parsed.ahead;
        let behind = parsed.behind;
        let mut files = parsed.into_file_entries();
        files.sort_by(|left, right| left.path_bytes.cmp(&right.path_bytes));
        let repo_name = repo_root
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| repo_root.to_string_lossy().to_string());
        let commits = GitCommitEntry::recent(&repo_root)?;
        let branches = GitBranchEntry::local(&repo_root, &branch)?;
        let remote_branches = GitBranchEntry::remote(&repo_root)?;
        let tags = GitTagEntry::list(&repo_root)?;
        let stashes = GitStashEntry::list(&repo_root)?;
        Ok(Self {
            path: requested_path,
            repo_root: repo_root.to_string_lossy().to_string(),
            repo_name,
            branch,
            upstream,
            ahead,
            behind,
            files,
            commits,
            branches,
            remote_branches,
            tags,
            stashes,
        })
    }
}

pub(crate) fn statuses(root: &Path, files: &[PathBuf]) -> Result<Vec<GitFileStatus>, GitError> {
    let (stdout, stderr, ok) = git_read_bytes(
        root,
        &[
            "status",
            "--porcelain=v2",
            "-z",
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
            GitFileStatus {
                path: file.to_string_lossy().into_owned(),
                branch: parsed.branch.clone(),
                ahead: parsed.ahead,
                behind: parsed.behind,
                has_upstream: parsed.has_upstream,
                file_status: parsed.file_status(&path_bytes(&target)),
                staged_count: parsed.staged_count,
                repo_root: repo_root.clone(),
            }
        })
        .collect())
}

pub(crate) fn config_path(root: &Path) -> Result<PathBuf, GitError> {
    let (stdout, stderr, ok) = git_read(
        root,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "config",
        ],
    )?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    let path = stdout.trim();
    if path.is_empty() {
        return Err(GitError("git config path is empty".to_string()));
    }
    Ok(PathBuf::from(path))
}

pub fn dirty_set(file: &Path) -> Result<(PathBuf, std::collections::HashSet<String>), GitError> {
    let root = repo_root(file)?;
    let (stdout, stderr, ok) = git_read_bytes(
        &root,
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
    )?;
    if !ok {
        return Err(GitError(stderr.trim().to_string()));
    }
    Ok((root, parse::changed_paths(&stdout)))
}

fn diff_text(root: &Path, target: &Path, cached: bool, ctx: u32) -> Result<String, GitError> {
    let uarg = format!("--unified={ctx}");
    let mut command = git_command(root);
    command.arg("diff");
    if cached {
        command.arg("--cached");
    }
    let output = command
        .arg(&uarg)
        .arg("--")
        .arg(target)
        .output()
        .map_err(|error| GitError(format!("failed to run git: {error}")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(GitError(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
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

fn staged_lineset(root: &Path, target: &Path) -> HashSet<u32> {
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
    target: &Path,
    staged: &HashSet<u32>,
) -> Result<Vec<DiffLine>, GitError> {
    if diff_text(root, target, true, 100_000)?.trim().is_empty() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(file).unwrap_or_default();
    let spans = crate::host::highlight::highlight_file(&content, file);
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

fn index_text(root: &Path, target: &Path) -> Result<String, GitError> {
    #[cfg(unix)]
    let spec = {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let mut bytes = vec![b':'];
        bytes.extend_from_slice(target.as_os_str().as_bytes());
        OsString::from_vec(bytes)
    };
    #[cfg(not(unix))]
    let spec = OsString::from(format!(":{}", target.to_string_lossy()));

    let output = git_command(root)
        .arg("show")
        .arg(spec)
        .output()
        .map_err(|error| GitError(format!("failed to run git: {error}")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Ok(String::new())
    }
}

pub fn diff_lines_with_content(
    root: &Path,
    file: &Path,
    content: &str,
) -> Result<Vec<DiffLine>, GitError> {
    let target = rel(root, file);
    let baseline = index_text(root, &target)?;
    let staged = staged_lineset(root, &target);
    let new_spans = crate::host::highlight::highlight_file(content, file);
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
                        .unwrap_or_else(|| crate::host::highlight::highlight_line(text, file)),
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
                    spans: crate::host::highlight::highlight_line(text, file),
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
                        .unwrap_or_else(|| crate::host::highlight::highlight_line(text, file)),
                });
                new_no += 1;
            }
        }
    }
    Ok(lines)
}

pub fn diff_lines(root: &Path, file: &Path) -> Result<Vec<DiffLine>, GitError> {
    let target = rel(root, file);
    let staged = staged_lineset(root, &target);

    let unstaged = diff_text(root, &target, false, 100_000)?;
    if unstaged.trim().is_empty() {
        if status_at(root, file)?.file_status == FileStatus::Untracked {
            let content = std::fs::read_to_string(file).unwrap_or_default();
            return diff_lines_with_content(root, file, &content);
        }
        return staged_only_lines(file, root, &target, &staged);
    }
    let ranges = parse::hunk_ranges(&diff_text(root, &target, false, 0)?);

    let new_spans = std::fs::read_to_string(file)
        .map(|c| crate::host::highlight::highlight_file(&c, file))
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
                    .unwrap_or_else(|| crate::host::highlight::highlight_line(&text, file)),
                _ => crate::host::highlight::highlight_line(&text, file),
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

pub fn commit_diff_lines(root: &Path, reference: &str) -> Result<Vec<DiffLine>, GitError> {
    let (stdout, stderr, ok) = git_read(
        root,
        &[
            "show",
            "--format=",
            "--find-renames",
            "--find-copies",
            "--unified=100000",
            reference,
        ],
    )?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    Ok(parse::parse_unified_diff(&stdout))
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

pub fn apply_hunk(root: &Path, file: &Path, index: u32, accept: bool) -> Result<(), GitError> {
    let target = rel(root, file);
    let diff = diff_text(root, &target, false, 0)?;
    if diff.trim().is_empty() {
        return Err(GitError("no unstaged changes for this file".into()));
    }
    let (header, hunks) = parse::hunk_patches(&diff);
    let body = hunks
        .get(index as usize)
        .ok_or_else(|| GitError("hunk index out of range".into()))?;
    let patch = format!("{header}{body}");
    git_apply(root, &patch, !accept)
}

fn simple(root: &Path, file: &Path, verb: &[&str]) -> Result<(), GitError> {
    let target = rel(root, file);
    let output = git_command(root)
        .args(verb)
        .arg(&target)
        .output()
        .map_err(|error| GitError(format!("failed to run git: {error}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_err(
            &String::from_utf8_lossy(&output.stdout),
            &String::from_utf8_lossy(&output.stderr),
        ))
    }
}

pub fn stage(root: &Path, file: &Path) -> Result<(), GitError> {
    simple(root, file, &["add", "--"])
}

pub fn unstage(root: &Path, file: &Path) -> Result<(), GitError> {
    simple(root, file, &["restore", "--staged", "--"])
}

pub fn discard(root: &Path, file: &Path) -> Result<(), GitError> {
    simple(root, file, &["restore", "--"])
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

pub fn fetch(file: &Path) -> Result<(), GitError> {
    let root = repo_root(file)?;
    let (stdout, stderr, ok) = git(&root, &["fetch", "--prune"])?;
    if ok {
        Ok(())
    } else {
        Err(git_err(&stdout, &stderr))
    }
}

pub fn pull(file: &Path) -> Result<(), GitError> {
    let root = repo_root(file)?;
    let (stdout, stderr, ok) = git(&root, &["pull", "--ff-only"])?;
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

pub fn stage_all(file: &Path) -> Result<(), GitError> {
    let root = repo_root(file)?;
    let (stdout, stderr, ok) = git(&root, &["add", "--all"])?;
    if ok {
        Ok(())
    } else {
        Err(git_err(&stdout, &stderr))
    }
}

#[cfg(test)]
pub(crate) mod test_repo {
    use super::*;

    pub fn run(dir: &Path, args: &[&str]) {
        let status = git_command(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    pub fn init() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        run(p, &["init", "-q", "-b", "main"]);
        run(p, &["config", "user.email", "t@example.com"]);
        run(p, &["config", "user.name", "Test"]);
        run(p, &["config", "commit.gpgsign", "false"]);
        dir
    }

    pub fn write(dir: &Path, rel: &str, contents: &str) -> PathBuf {
        let path = dir.join(rel);
        std::fs::write(&path, contents).unwrap();
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_command_scrubs_all_local_env_vars() {
        use std::ffi::OsStr;
        let cmd = git_command(Path::new("."));
        let removed: HashSet<&OsStr> = cmd
            .get_envs()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k)
            .collect();
        let vars = local_env_vars();
        assert!(!vars.is_empty(), "local_env_vars must not be empty");
        for var in vars {
            assert!(
                removed.contains(OsStr::new(var.as_str())),
                "git_command must scrub {var} so ambient GIT_* cannot redirect the runner"
            );
        }
        for key in [
            "GIT_DIR",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_CONFIG",
        ] {
            assert!(
                vars.iter().any(|v| v == key),
                "{key} must appear in git's local-env-vars"
            );
        }
    }

    #[test]
    fn repo_root_resolves_toplevel() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "hi");
        let root = repo_root(&file).unwrap();
        assert_eq!(
            root.canonicalize().unwrap(),
            repo.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn repo_root_errors_outside_repo() {
        let dir = tempfile::tempdir().unwrap();
        let file = test_repo::write(dir.path(), "loose.txt", "x");
        assert!(repo_root(&file).is_err());
    }

    #[test]
    fn detects_repository_marker_in_ancestor() {
        let repo = test_repo::init();
        let nested = repo.path().join("notes/projects");
        std::fs::create_dir_all(&nested).unwrap();
        let file = test_repo::write(&nested, "plan.md", "# Plan");
        assert!(has_repository(&file));

        let outside = tempfile::tempdir().unwrap();
        assert!(!has_repository(&outside.path().join("note.md")));
    }

    #[test]
    fn dirty_set_lists_modified_and_untracked_not_clean() {
        let repo = test_repo::init();
        let _clean = test_repo::write(repo.path(), "clean.txt", "x\n");
        let modified = test_repo::write(repo.path(), "mod.txt", "one\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "mod.txt", "two\n");
        test_repo::write(repo.path(), "new.txt", "n\n");

        let (root, set) = dirty_set(&modified).unwrap();
        assert_eq!(
            root.canonicalize().unwrap(),
            repo.path().canonicalize().unwrap()
        );
        assert!(set.contains("mod.txt"));
        assert!(set.contains("new.txt"));
        assert!(!set.contains("clean.txt"));
    }

    #[test]
    fn repository_loads_changes_history_and_branches() {
        let repo = test_repo::init();
        test_repo::write(repo.path(), "tracked.txt", "one\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "initial"]);
        test_repo::run(repo.path(), &["branch", "feature"]);
        test_repo::run(repo.path(), &["tag", "v1.0.0"]);
        test_repo::run(
            repo.path(),
            &["update-ref", "refs/remotes/origin/main", "HEAD"],
        );
        test_repo::write(repo.path(), "tracked.txt", "two\n");
        test_repo::write(repo.path(), "untracked.txt", "new\n");

        let repository = GitRepositorySnapshot::load(repo.path()).unwrap();

        assert_eq!(repository.branch, "main");
        assert!(
            repository
                .files
                .iter()
                .any(|entry| entry.path == "tracked.txt" && entry.unstaged)
        );
        assert!(
            repository
                .files
                .iter()
                .any(|entry| entry.path == "untracked.txt" && entry.status == FileStatus::Untracked)
        );
        assert_eq!(repository.commits[0].summary, "initial");
        assert!(
            repository
                .branches
                .iter()
                .any(|branch| branch.name == "main" && branch.current)
        );
        assert!(
            repository
                .branches
                .iter()
                .any(|branch| branch.name == "feature")
        );
        assert!(
            repository
                .remote_branches
                .iter()
                .any(|branch| branch.name == "origin/main")
        );
        assert!(repository.tags.iter().any(|tag| tag.name == "v1.0.0"));
        assert!(repository.stashes.is_empty());
    }

    #[test]
    fn commit_diff_lines_separate_files_in_the_selected_commit() {
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::write(repo.path(), "b.txt", "two\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "initial"]);

        let lines = commit_diff_lines(repo.path(), "HEAD").unwrap();
        let files = lines
            .iter()
            .filter(|line| matches!(line.kind, DiffKind::Hunk))
            .filter_map(|line| line.spans.first())
            .map(|span| span.text.as_str())
            .filter(|text| !text.starts_with("@@"))
            .collect::<Vec<_>>();

        assert_eq!(files, vec!["a.txt", "b.txt"]);
        assert!(lines.iter().any(|line| line.kind == DiffKind::Add));
    }

    #[test]
    fn branch_operations_create_and_delete_a_branch() {
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "initial"]);

        GitOperation::CreateBranch {
            branch: "feature".to_string(),
            start_point: "main".to_string(),
        }
        .run(repo.path())
        .unwrap();
        assert!(
            GitBranchEntry::local(repo.path(), "main")
                .unwrap()
                .iter()
                .any(|branch| branch.name == "feature")
        );

        GitOperation::DeleteBranch {
            branch: "feature".to_string(),
        }
        .run(repo.path())
        .unwrap();
        assert!(
            GitBranchEntry::local(repo.path(), "main")
                .unwrap()
                .iter()
                .all(|branch| branch.name != "feature")
        );
    }

    #[test]
    fn stash_operations_preserve_and_restore_tracked_and_untracked_changes() {
        let repo = test_repo::init();
        test_repo::write(repo.path(), "tracked.txt", "one\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "initial"]);
        test_repo::write(repo.path(), "tracked.txt", "two\n");
        test_repo::write(repo.path(), "untracked.txt", "new\n");

        GitOperation::StashPush.run(repo.path()).unwrap();

        let repository = GitRepositorySnapshot::load(repo.path()).unwrap();
        assert!(repository.files.is_empty());
        assert_eq!(repository.stashes.len(), 1);
        let reference = repository.stashes[0].reference.clone();

        GitOperation::StashPop { reference }
            .run(repo.path())
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
            "two\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.path().join("untracked.txt")).unwrap(),
            "new\n"
        );
        assert!(
            GitRepositorySnapshot::load(repo.path())
                .unwrap()
                .stashes
                .is_empty()
        );

        GitOperation::StashPush.run(repo.path()).unwrap();
        let reference = GitRepositorySnapshot::load(repo.path()).unwrap().stashes[0]
            .reference
            .clone();
        GitOperation::StashDrop { reference }
            .run(repo.path())
            .unwrap();
        assert!(
            GitRepositorySnapshot::load(repo.path())
                .unwrap()
                .stashes
                .is_empty()
        );
    }

    #[test]
    fn cherry_pick_and_revert_apply_selected_commit() {
        let repo = test_repo::init();
        test_repo::write(repo.path(), "base.txt", "base\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "base"]);
        test_repo::run(repo.path(), &["switch", "-qc", "feature"]);
        test_repo::write(repo.path(), "feature.txt", "feature\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "feature"]);
        let (commit, _, ok) = git_read(repo.path(), &["rev-parse", "HEAD"]).unwrap();
        assert!(ok);
        let commit = commit.trim().to_string();
        test_repo::run(repo.path(), &["switch", "main"]);

        GitOperation::CherryPick {
            commit: commit.clone(),
        }
        .run(repo.path())
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.path().join("feature.txt")).unwrap(),
            "feature\n"
        );

        GitOperation::Revert { commit }.run(repo.path()).unwrap();
        assert!(!repo.path().join("feature.txt").exists());
    }

    #[test]
    fn rebase_moves_current_branch_onto_selected_branch() {
        let repo = test_repo::init();
        test_repo::write(repo.path(), "base.txt", "base\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "base"]);
        test_repo::run(repo.path(), &["branch", "feature"]);
        test_repo::write(repo.path(), "main.txt", "main\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "main"]);
        test_repo::run(repo.path(), &["switch", "feature"]);
        test_repo::write(repo.path(), "feature.txt", "feature\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "feature"]);

        GitOperation::Rebase {
            branch: "main".to_string(),
        }
        .run(repo.path())
        .unwrap();

        let (_, _, ancestor) = git_read(
            repo.path(),
            &["merge-base", "--is-ancestor", "main", "HEAD"],
        )
        .unwrap();
        assert!(ancestor);
        assert!(repo.path().join("main.txt").exists());
        assert!(repo.path().join("feature.txt").exists());
    }

    #[test]
    fn merge_and_fast_forward_integrate_selected_branch() {
        let merge_repo = test_repo::init();
        test_repo::write(merge_repo.path(), "base.txt", "base\n");
        test_repo::run(merge_repo.path(), &["add", "."]);
        test_repo::run(merge_repo.path(), &["commit", "-qm", "base"]);
        test_repo::run(merge_repo.path(), &["switch", "-qc", "feature"]);
        test_repo::write(merge_repo.path(), "feature.txt", "feature\n");
        test_repo::run(merge_repo.path(), &["add", "."]);
        test_repo::run(merge_repo.path(), &["commit", "-qm", "feature"]);
        test_repo::run(merge_repo.path(), &["switch", "main"]);

        GitOperation::Merge {
            branch: "feature".to_string(),
        }
        .run(merge_repo.path())
        .unwrap();
        assert!(merge_repo.path().join("feature.txt").exists());

        let ff_repo = test_repo::init();
        test_repo::write(ff_repo.path(), "base.txt", "base\n");
        test_repo::run(ff_repo.path(), &["add", "."]);
        test_repo::run(ff_repo.path(), &["commit", "-qm", "base"]);
        test_repo::run(ff_repo.path(), &["switch", "-qc", "feature"]);
        test_repo::write(ff_repo.path(), "feature.txt", "feature\n");
        test_repo::run(ff_repo.path(), &["add", "."]);
        test_repo::run(ff_repo.path(), &["commit", "-qm", "feature"]);
        let (feature_head, _, ok) = git_read(ff_repo.path(), &["rev-parse", "HEAD"]).unwrap();
        assert!(ok);
        test_repo::run(ff_repo.path(), &["switch", "main"]);

        GitOperation::FastForward {
            branch: "feature".to_string(),
        }
        .run(ff_repo.path())
        .unwrap();
        let (head, _, ok) = git_read(ff_repo.path(), &["rev-parse", "HEAD"]).unwrap();
        assert!(ok);
        assert_eq!(head.trim(), feature_head.trim());
    }

    #[test]
    fn status_reports_modified_then_staged() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "a.txt", "two\n");

        assert_eq!(status(&file).unwrap().file_status, FileStatus::Modified);
        stage(repo.path(), &file).unwrap();
        assert_eq!(status(&file).unwrap().file_status, FileStatus::Staged);
    }

    #[test]
    fn status_reports_nested_untracked_file() {
        let repo = test_repo::init();
        std::fs::create_dir(repo.path().join("nested")).unwrap();
        let file = test_repo::write(repo.path(), "nested/new.txt", "new\n");

        assert_eq!(status(&file).unwrap().file_status, FileStatus::Untracked);
    }

    #[test]
    fn status_batch_reports_each_requested_path() {
        let repo = test_repo::init();
        let modified = test_repo::write(repo.path(), "modified.txt", "one\n");
        let staged = test_repo::write(repo.path(), "staged.txt", "one\n");
        test_repo::run(repo.path(), &["add", "."]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "modified.txt", "two\n");
        test_repo::write(repo.path(), "staged.txt", "two\n");
        test_repo::run(repo.path(), &["add", "staged.txt"]);
        let modified_path = modified.to_string_lossy().into_owned();
        let staged_path = staged.to_string_lossy().into_owned();

        let events = statuses(repo.path(), &[modified, staged]).unwrap();

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].path, modified_path);
        assert_eq!(events[1].path, staged_path);
        assert_eq!(events[0].file_status, FileStatus::Modified);
        assert_eq!(events[1].file_status, FileStatus::Staged);
    }

    #[test]
    fn repository_load_preserves_special_pathnames() {
        let repo = test_repo::init();
        let name = "tab\tline\nquote\"slash\\name.txt";
        test_repo::write(repo.path(), name, "new\n");

        let repository = GitRepositorySnapshot::load(repo.path()).unwrap();

        assert!(repository.files.iter().any(|entry| entry.path == name));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_path_round_trips_through_repository_actions() {
        use std::os::unix::ffi::OsStringExt;

        let repo = test_repo::init();
        let raw_path = b"invalid-\x80-name.txt".to_vec();
        let file = repo.path().join(OsString::from_vec(raw_path.clone()));
        std::fs::write(&file, "one\n").unwrap();
        stage(repo.path(), &file).unwrap();
        commit(&file, "initial").unwrap();
        std::fs::write(&file, "two\n").unwrap();

        let repository = GitRepositorySnapshot::load(repo.path()).unwrap();
        let entry = repository
            .files
            .iter()
            .find(|entry| entry.path_bytes == raw_path)
            .unwrap();
        let request_path = RequestPath::new(&entry.path, &entry.path_bytes).resolve(repo.path());

        assert_eq!(request_path, file);
        assert!(!diff_lines(repo.path(), &request_path).unwrap().is_empty());
        stage(repo.path(), &request_path).unwrap();
        assert_eq!(
            status(&request_path).unwrap().file_status,
            FileStatus::Staged
        );
        unstage(repo.path(), &request_path).unwrap();
        assert_eq!(
            status(&request_path).unwrap().file_status,
            FileStatus::Modified
        );
        discard(repo.path(), &request_path).unwrap();
        assert_eq!(std::fs::read_to_string(request_path).unwrap(), "one\n");
    }

    #[test]
    fn background_status_does_not_refresh_index() {
        use std::fs::{FileTimes, OpenOptions};
        use std::time::{Duration, SystemTime};

        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let index = repo.path().join(".git/index");
        let before = std::fs::read(&index).unwrap();
        OpenOptions::new()
            .write(true)
            .open(&file)
            .unwrap()
            .set_times(FileTimes::new().set_modified(SystemTime::now() + Duration::from_secs(3600)))
            .unwrap();

        assert_eq!(status(&file).unwrap().file_status, FileStatus::Clean);

        assert_eq!(std::fs::read(index).unwrap(), before);
    }

    #[test]
    fn diff_lines_show_added_and_removed() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "a.txt", "two\n");

        let lines = diff_lines(repo.path(), &file).unwrap();
        assert!(lines.iter().any(|l| matches!(l.kind, DiffKind::Add)));
        assert!(lines.iter().any(|l| matches!(l.kind, DiffKind::Remove)));
    }

    #[test]
    fn diff_lines_show_untracked_file_as_additions() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "new.txt", "one\ntwo\n");

        let lines = diff_lines(repo.path(), &file).unwrap();

        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| line.kind == DiffKind::Add));
        assert_eq!(lines[0].new_no, Some(1));
        assert_eq!(lines[1].new_no, Some(2));
    }

    #[test]
    fn repository_root_keeps_a_changed_nested_repository_as_the_selected_path() {
        let repo = test_repo::init();
        let nested = repo.path().join("client");
        std::fs::create_dir(&nested).unwrap();
        test_repo::run(&nested, &["init", "-q", "-b", "main"]);
        test_repo::run(&nested, &["config", "user.email", "t@example.com"]);
        test_repo::run(&nested, &["config", "user.name", "Test"]);
        test_repo::write(&nested, "a.txt", "one\n");
        test_repo::run(&nested, &["add", "a.txt"]);
        test_repo::run(&nested, &["commit", "-qm", "initial"]);
        test_repo::run(repo.path(), &["add", "client"]);
        test_repo::run(repo.path(), &["commit", "-qm", "add client"]);
        test_repo::write(&nested, "a.txt", "two\n");
        test_repo::run(&nested, &["add", "a.txt"]);
        test_repo::run(&nested, &["commit", "-qm", "advance client"]);

        let lines = diff_lines(repo.path(), &nested).unwrap();
        assert!(!lines.is_empty());
        assert_eq!(
            status_at(repo.path(), &nested).unwrap().file_status,
            FileStatus::Modified
        );

        stage(repo.path(), &nested).unwrap();
        assert_eq!(
            status_at(repo.path(), &nested).unwrap().file_status,
            FileStatus::Staged
        );
    }

    #[test]
    fn diff_lines_with_content_reads_unsaved_buffer() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\ntwo\nthree\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);

        let lines = diff_lines_with_content(repo.path(), &file, "one\nchanged\nthree\n").unwrap();

        assert!(
            lines
                .iter()
                .any(|line| { matches!(line.kind, DiffKind::Remove) && line.old_no == Some(2) })
        );
        assert!(
            lines
                .iter()
                .any(|line| { matches!(line.kind, DiffKind::Add) && line.new_no == Some(2) })
        );
        assert_eq!(std::fs::read_to_string(file).unwrap(), "one\ntwo\nthree\n");
    }

    #[test]
    fn handles_path_with_spaces_and_metachars() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a b; rm.txt", "one\n");
        stage(repo.path(), &file).unwrap();
        assert_eq!(status(&file).unwrap().file_status, FileStatus::Staged);
    }

    #[test]
    fn unstage_returns_to_modified() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "a.txt", "two\n");
        stage(repo.path(), &file).unwrap();
        unstage(repo.path(), &file).unwrap();
        assert_eq!(status(&file).unwrap().file_status, FileStatus::Modified);
    }

    #[test]
    fn discard_reverts_working_tree() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "a.txt", "two\n");
        discard(repo.path(), &file).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "one\n");
    }

    #[test]
    fn commit_clears_staged_and_advances_head() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        stage(repo.path(), &file).unwrap();
        commit(&file, "add a").unwrap();
        assert_eq!(status(&file).unwrap().staged_count, 0);
        let (log, _, ok) = git(repo.path(), &["log", "--oneline"]).unwrap();
        assert!(ok && log.contains("add a"));
    }

    #[test]
    fn commit_with_nothing_staged_errors() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        assert!(commit(&file, "noop").is_err());
    }

    #[test]
    fn push_updates_bare_remote() {
        let remote = tempfile::tempdir().unwrap();
        test_repo::run(remote.path(), &["init", "-q", "--bare"]);
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        stage(repo.path(), &file).unwrap();
        commit(&file, "init").unwrap();
        test_repo::run(
            repo.path(),
            &["remote", "add", "origin", remote.path().to_str().unwrap()],
        );
        test_repo::run(repo.path(), &["push", "-u", "origin", "main"]);

        test_repo::write(repo.path(), "a.txt", "two\n");
        stage(repo.path(), &file).unwrap();
        commit(&file, "second").unwrap();
        push(&file).unwrap();

        let (log, _, ok) = git(remote.path(), &["log", "--oneline", "main"]).unwrap();
        assert!(ok && log.contains("second"));
    }

    #[test]
    fn apply_hunk_accept_stages_then_reject_reverts() {
        let repo = test_repo::init();
        let file = test_repo::write(
            repo.path(),
            "a.txt",
            "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\n",
        );
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(
            repo.path(),
            "a.txt",
            "L1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nL10\n",
        );

        apply_hunk(repo.path(), &file, 0, true).unwrap();
        assert_eq!(
            status(&file).unwrap().file_status,
            FileStatus::StagedModified
        );

        apply_hunk(repo.path(), &file, 0, false).unwrap();
        let content = std::fs::read_to_string(&file).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.first().copied(), Some("L1"));
        assert_eq!(lines.last().copied(), Some("l10"));
    }

    #[test]
    fn diff_lines_marks_accepted_hunk_staged_unstaged_remains() {
        let repo = test_repo::init();
        let body = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\nl11\nl12\n";
        let file = test_repo::write(repo.path(), "a.txt", body);
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(
            repo.path(),
            "a.txt",
            "L1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\nl11\nL12\n",
        );

        apply_hunk(repo.path(), &file, 0, true).unwrap();
        let lines = diff_lines(repo.path(), &file).unwrap();
        assert!(lines.iter().any(|l| matches!(l.kind, DiffKind::Staged)));
        assert!(
            lines
                .iter()
                .any(|l| matches!(l.kind, DiffKind::Add | DiffKind::Remove))
        );
    }

    #[test]
    fn close_changes_are_independent_hunks() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "l1\nl2\nl3\nl4\nl5\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "a.txt", "X1\nl2\nX3\nl4\nl5\n");

        let hunks: std::collections::HashSet<u32> = diff_lines(repo.path(), &file)
            .unwrap()
            .iter()
            .filter_map(|l| l.hunk)
            .collect();
        assert_eq!(hunks.len(), 2, "expected 2 separate hunks, got {hunks:?}");

        apply_hunk(repo.path(), &file, 0, true).unwrap();
        let removes: Vec<_> = diff_lines(repo.path(), &file)
            .unwrap()
            .into_iter()
            .filter(|l| matches!(l.kind, DiffKind::Remove))
            .collect();
        assert_eq!(removes.len(), 1);
        assert_eq!(removes[0].old_no, Some(3));
    }

    #[test]
    fn deny_hunk_restores_line_and_clears_its_highlight() {
        let repo = test_repo::init();
        let filler = "    f();\n".repeat(10);
        let head = format!("fn greet() {{\n    a();\n}}\n{filler}fn main() {{\n    done();\n}}\n");
        let file = test_repo::write(repo.path(), "a.rs", &head);
        test_repo::run(repo.path(), &["add", "a.rs"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let work = format!("fn greet() {{\n    B();\n}}\n{filler}fn main() {{\n}}\n");
        test_repo::write(repo.path(), "a.rs", &work);

        apply_hunk(repo.path(), &file, 1, false).unwrap();

        assert!(std::fs::read_to_string(&file).unwrap().contains("done();"));
        let after = diff_lines(repo.path(), &file).unwrap();
        let removes: Vec<_> = after
            .iter()
            .filter(|l| matches!(l.kind, DiffKind::Remove))
            .collect();
        assert_eq!(removes.len(), 1);
        assert_eq!(removes[0].old_no, Some(2));
    }

    #[test]
    fn diff_lines_fully_staged_shows_code_without_signs() {
        let repo = test_repo::init();
        let body = "l1\nl2\nl3\nl4\nl5\n";
        let file = test_repo::write(repo.path(), "a.txt", body);
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "a.txt", "L1\nl2\nl3\nl4\nl5\n");
        stage(repo.path(), &file).unwrap();

        let lines = diff_lines(repo.path(), &file).unwrap();
        assert_eq!(lines.len(), 5);
        assert!(lines.iter().any(|l| matches!(l.kind, DiffKind::Staged)));
        assert!(
            !lines
                .iter()
                .any(|l| matches!(l.kind, DiffKind::Add | DiffKind::Remove))
        );
    }
}
