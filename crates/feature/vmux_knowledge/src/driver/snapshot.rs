use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct RawNote {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct RawSnapshot {
    pub root: PathBuf,
    pub files: Vec<RawNote>,
}

impl RawSnapshot {
    const MAX_NOTES: usize = 2_048;
    const MAX_NOTE_BYTES: u64 = 2 * 1024 * 1024;
    const MAX_DEPTH: usize = 16;

    pub fn capture(root: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(root)?;
        let root = root.canonicalize()?;
        let mut paths = Vec::new();
        Self::collect_files(&root, 0, &mut paths)?;
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            let file = match std::fs::File::open(&path) {
                Ok(file) => file,
                Err(_) => continue,
            };
            let mut text = String::new();
            if file
                .take(Self::MAX_NOTE_BYTES)
                .read_to_string(&mut text)
                .is_err()
            {
                continue;
            }
            files.push(RawNote { path, text });
        }
        Ok(Self { root, files })
    }

    fn collect_files(directory: &Path, depth: usize, paths: &mut Vec<PathBuf>) -> io::Result<()> {
        if depth > Self::MAX_DEPTH || paths.len() >= Self::MAX_NOTES {
            return Ok(());
        }
        let mut entries = std::fs::read_dir(directory)?.flatten().collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());
        for entry in entries {
            if paths.len() >= Self::MAX_NOTES {
                break;
            }
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                Self::collect_files(&path, depth + 1, paths)?;
            } else if kind.is_file() && Self::is_markdown(&path) {
                paths.push(path);
            }
        }
        Ok(())
    }

    fn is_markdown(path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("md")
                    || extension.eq_ignore_ascii_case("markdown")
                    || extension.eq_ignore_ascii_case("mdx")
            })
    }
}
