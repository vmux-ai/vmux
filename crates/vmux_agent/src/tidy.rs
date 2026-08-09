use bevy::prelude::*;
use std::path::PathBuf;

/// Marker on a follow-pane: the clean previews awaiting the user's answer to the in-UI
/// tidy banner (`FileTidyPromptEvent` shown, `FileTidyActionEvent` pending).
#[derive(Component)]
pub(crate) struct PendingTidy {
    pub closable: Vec<Entity>,
}

/// Absolute filesystem path from a `file://` URL: strips the scheme and any
/// `#fragment`, then percent-decodes. `None` for non-`file:` or empty paths.
pub(crate) fn path_from_file_url(url: &str) -> Option<PathBuf> {
    let rest = url
        .strip_prefix("file://")
        .or_else(|| url.strip_prefix("file:"))?;
    let no_frag = rest.split('#').next().unwrap_or(rest);
    let decoded = percent_decode(no_frag);
    if decoded.is_empty() {
        return None;
    }
    Some(PathBuf::from(decoded))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Given `(stack, last_activated, changed)` for every file preview in a pane and
/// the tidy threshold, the stacks to close: clean, not the active (max
/// last_activated) one. Empty if at/below threshold or nothing is closable.
pub(crate) fn decide_closable(stacks: &[(Entity, i64, bool)], max: usize) -> Vec<Entity> {
    if stacks.len() <= max {
        return Vec::new();
    }
    let active = stacks
        .iter()
        .max_by_key(|(_, ts, _)| *ts)
        .map(|(s, _, _)| *s);
    stacks
        .iter()
        .filter(|(s, _, changed)| Some(*s) != active && !changed)
        .map(|(s, _, _)| *s)
        .collect()
}

/// Whether `abs` is git-changed. Memoizes `(repo_root, changed_set)` per repo in
/// `repos`. Files outside any repo (or that error) are treated as clean.
pub(crate) fn is_changed(
    abs: &std::path::Path,
    repos: &mut Vec<(PathBuf, std::collections::HashSet<String>)>,
) -> bool {
    let abs = abs.canonicalize().unwrap_or_else(|_| abs.to_path_buf());
    if let Some((root, set)) = repos.iter().find(|(r, _)| abs.starts_with(r)) {
        return set.contains(&rel_str(root, &abs));
    }
    match vmux_git::runner::dirty_set(&abs) {
        Ok((root, set)) => {
            let changed = set.contains(&rel_str(&root, &abs));
            repos.push((root, set));
            changed
        }
        Err(_) => false,
    }
}

fn rel_str(root: &std::path::Path, abs: &std::path::Path) -> String {
    abs.strip_prefix(root)
        .map(|r| r.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "tidy.test.rs"]
mod tests;
