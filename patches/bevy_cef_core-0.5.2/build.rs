fn main() {
    #[cfg(target_os = "windows")]
    windows::copy_cef_files();
}

#[cfg(target_os = "windows")]
mod windows {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};

    const RUNTIME_EXTENSIONS: &[&str; 6] = &["dll", "lib", "pak", "dat", "bin", "json"];

    const RENDER_PROCESS_BINARY: &str = "bevy_cef_render_process.exe";

    const RUNTIME_SUBDIRS: &[&str] = &["locales", "Resources"];

    pub fn copy_cef_files() {
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-env-changed=CEF_DIR");
        println!("cargo:rerun-if-env-changed=USERPROFILE");

        let Some(cef_dir) = find_cef_dir() else {
            return;
        };

        let target_dir = find_target_profile_dir();
        let examples_dir = target_dir.join("examples");

        copy_cef_runtime_files(&cef_dir, &target_dir);
        copy_render_process_binary(&cef_dir, &target_dir);

        fs::create_dir_all(&examples_dir).unwrap();
        copy_cef_runtime_files(&target_dir, &examples_dir);
        copy_render_process_binary(&target_dir, &examples_dir);
    }

    fn find_cef_dir() -> Option<PathBuf> {
        if let Ok(dir) = env::var("CEF_DIR") {
            let p = PathBuf::from(dir.trim());
            if p.is_dir() {
                println!("cargo:rerun-if-changed={}", p.display());
                return Some(p);
            }
        }

        let home = env::var("USERPROFILE").ok()?;
        let cef_dir = PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("cef");
        println!("cargo:rerun-if-changed={}", cef_dir.display());
        if !cef_dir.exists() {
            return None;
        }
        Some(cef_dir)
    }

    fn find_target_profile_dir() -> PathBuf {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let mut dir = out_dir.as_path();
        loop {
            if dir.file_name().map(|n| n == "build").unwrap_or(false) {
                return dir.parent().unwrap().to_path_buf();
            }
            dir = dir
                .parent()
                .expect("Could not find target profile directory from OUT_DIR");
        }
    }

    fn is_runtime_file(path: &Path) -> bool {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        RUNTIME_EXTENSIONS.contains(&ext.as_str())
    }

    fn copy_render_process_binary(src: &Path, dst: &Path) {
        let binary = src.join(RENDER_PROCESS_BINARY);
        let binary_in_bin = src.join("bin").join(RENDER_PROCESS_BINARY);
        let binary = if binary.exists() {
            binary
        } else if binary_in_bin.exists() {
            binary_in_bin
        } else {
            return;
        };
        let dest = dst.join(RENDER_PROCESS_BINARY);
        copy_if_newer(&binary, &dest).unwrap_or_else(|e| {
            panic!("Failed to copy {:?} to {:?}: {}", binary, dest, e);
        });
    }

    fn copy_if_newer(src: &Path, dest: &Path) -> std::io::Result<()> {
        if dest.exists() {
            let src_modified = fs::metadata(src)?.modified()?;
            let dst_modified = fs::metadata(dest)?.modified()?;
            if dst_modified >= src_modified {
                return Ok(());
            }
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dest)?;
        Ok(())
    }

    fn copy_cef_runtime_files(src: &Path, dst: &Path) {
        let entries = fs::read_dir(src).unwrap_or_else(|e| {
            panic!("Failed to read CEF directory {:?}: {}", src, e);
        });

        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            if path.is_dir() {
                if RUNTIME_SUBDIRS
                    .iter()
                    .any(|d| name.eq_ignore_ascii_case(d))
                {
                    let dest_dir = dst.join(&file_name);
                    fs::create_dir_all(&dest_dir).unwrap();
                    copy_tree_preserving_runtime(&path, &dest_dir);
                }
            } else if is_runtime_file(&path) {
                let dest = dst.join(&file_name);
                copy_if_newer(&path, &dest).unwrap_or_else(|e| {
                    panic!("Failed to copy {:?} to {:?}: {}", path, dest, e);
                });
            }
        }
    }

    fn copy_tree_preserving_runtime(src: &Path, dst: &Path) {
        for entry in fs::read_dir(src).unwrap_or_else(|e| {
            panic!("Failed to read directory {:?}: {}", src, e);
        }) {
            let entry = entry.unwrap();
            let path = entry.path();
            let file_name = entry.file_name();
            if path.is_dir() {
                let dest_dir = dst.join(&file_name);
                fs::create_dir_all(&dest_dir).unwrap();
                copy_tree_preserving_runtime(&path, &dest_dir);
            } else {
                let dest = dst.join(&file_name);
                copy_if_newer(&path, &dest).unwrap_or_else(|e| {
                    panic!("Failed to copy {:?} to {:?}: {}", path, dest, e);
                });
            }
        }
    }
}
