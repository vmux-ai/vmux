use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::event::*;
use crate::parse;

#[derive(Debug, Clone)]
pub struct GitError(pub String);

fn git(root: &Path, args: &[&str]) -> Result<(String, String, bool), GitError> {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| GitError(format!("failed to run git: {e}")))?;
    Ok((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    ))
}

fn git_err(stdout: &str, stderr: &str) -> GitError {
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
    let target = rel(&root, file);
    let (stdout, stderr, ok) = git(&root, &["status", "--porcelain=v2", "--branch"])?;
    if !ok {
        return Err(GitError(stderr.trim().to_string()));
    }
    let p = parse::parse_porcelain_v2(&stdout, &target);
    Ok(GitStatusEvent {
        branch: p.branch,
        ahead: p.ahead,
        behind: p.behind,
        has_upstream: p.has_upstream,
        file_status: p.file_status,
        staged_count: p.staged_count,
        repo_root: root.to_string_lossy().into_owned(),
    })
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
    let mut child = Command::new("git")
        .current_dir(root)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0")
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
pub(crate) mod test_repo {
    use super::*;

    pub fn run(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(dir)
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
    fn status_reports_modified_then_staged() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "a.txt", "two\n");

        assert_eq!(status(&file).unwrap().file_status, FileStatus::Modified);
        stage(&file).unwrap();
        assert_eq!(status(&file).unwrap().file_status, FileStatus::Staged);
    }

    #[test]
    fn diff_lines_show_added_and_removed() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "a.txt", "two\n");

        let lines = diff_lines(&file).unwrap();
        assert!(lines.iter().any(|l| matches!(l.kind, DiffKind::Add)));
        assert!(lines.iter().any(|l| matches!(l.kind, DiffKind::Remove)));
    }

    #[test]
    fn handles_path_with_spaces_and_metachars() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a b; rm.txt", "one\n");
        stage(&file).unwrap();
        assert_eq!(status(&file).unwrap().file_status, FileStatus::Staged);
    }

    #[test]
    fn unstage_returns_to_modified() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "a.txt", "two\n");
        stage(&file).unwrap();
        unstage(&file).unwrap();
        assert_eq!(status(&file).unwrap().file_status, FileStatus::Modified);
    }

    #[test]
    fn discard_reverts_working_tree() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "a.txt", "two\n");
        discard(&file).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "one\n");
    }

    #[test]
    fn commit_clears_staged_and_advances_head() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        stage(&file).unwrap();
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
        stage(&file).unwrap();
        commit(&file, "init").unwrap();
        test_repo::run(
            repo.path(),
            &["remote", "add", "origin", remote.path().to_str().unwrap()],
        );
        test_repo::run(repo.path(), &["push", "-u", "origin", "main"]);

        test_repo::write(repo.path(), "a.txt", "two\n");
        stage(&file).unwrap();
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

        apply_hunk(&file, 0, true).unwrap();
        assert_eq!(
            status(&file).unwrap().file_status,
            FileStatus::StagedModified
        );

        apply_hunk(&file, 0, false).unwrap();
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

        apply_hunk(&file, 0, true).unwrap();
        let lines = diff_lines(&file).unwrap();
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
        // change line 1 and line 3 — only 2 lines apart (would merge under -U3)
        test_repo::write(repo.path(), "a.txt", "X1\nl2\nX3\nl4\nl5\n");

        let hunks: std::collections::HashSet<u32> = diff_lines(&file)
            .unwrap()
            .iter()
            .filter_map(|l| l.hunk)
            .collect();
        assert_eq!(hunks.len(), 2, "expected 2 separate hunks, got {hunks:?}");

        // accepting hunk 0 (line 1) must not touch line 3
        apply_hunk(&file, 0, true).unwrap();
        let removes: Vec<_> = diff_lines(&file)
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

        apply_hunk(&file, 1, false).unwrap();

        assert!(std::fs::read_to_string(&file).unwrap().contains("done();"));
        let after = diff_lines(&file).unwrap();
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
        stage(&file).unwrap();

        let lines = diff_lines(&file).unwrap();
        assert_eq!(lines.len(), 5);
        assert!(lines.iter().any(|l| matches!(l.kind, DiffKind::Staged)));
        assert!(
            !lines
                .iter()
                .any(|l| matches!(l.kind, DiffKind::Add | DiffKind::Remove))
        );
    }
}
