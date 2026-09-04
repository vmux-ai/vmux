use std::path::{Path, PathBuf};

use super::runtime::PreparedRuntime;

const RUNTIME_FINGERPRINT: &str = ".extension-runtime-v2";
const LEGACY_RUNTIME_MARKER: &str = ".stable-runtime-v1";
const SERVICE_WORKER_DIR: &str = "Service Worker";

pub(crate) struct ServiceWorkerCache {
    default_dir: PathBuf,
}

impl ServiceWorkerCache {
    pub(crate) fn of(cef_profile: &Path) -> Self {
        Self {
            default_dir: cef_profile.join("Default"),
        }
    }

    pub(crate) fn reconcile(&self, prepared: &[PreparedRuntime]) -> Result<(), String> {
        if prepared.is_empty() {
            return Ok(());
        }
        let fingerprint = Self::fingerprint(prepared);
        let marker = self.default_dir.join(RUNTIME_FINGERPRINT);
        if std::fs::read_to_string(&marker).ok().as_deref() == Some(fingerprint.as_str()) {
            return Ok(());
        }
        self.clear()?;
        std::fs::create_dir_all(&self.default_dir).map_err(|error| error.to_string())?;
        let _ = std::fs::remove_file(self.default_dir.join(LEGACY_RUNTIME_MARKER));
        std::fs::write(&marker, fingerprint).map_err(|error| error.to_string())
    }

    fn fingerprint(prepared: &[PreparedRuntime]) -> String {
        let mut lines = prepared
            .iter()
            .map(|runtime| format!("{} {}", runtime.extension_id, runtime.runtime_hash))
            .collect::<Vec<_>>();
        lines.sort();
        lines.join("\n")
    }

    fn clear(&self) -> Result<(), String> {
        let service_workers = self.default_dir.join(SERVICE_WORKER_DIR);
        if !service_workers.exists() {
            return Ok(());
        }
        let stale = service_workers.with_file_name(format!(
            "{SERVICE_WORKER_DIR}.vmux-stale-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::rename(&service_workers, &stale).map_err(|error| error.to_string())?;
        let _ = std::thread::Builder::new()
            .name("extension-cache-cleanup".into())
            .spawn(move || {
                let _ = std::fs::remove_dir_all(stale);
            });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Profile {
        dir: tempfile::TempDir,
    }

    impl Profile {
        fn with_cached_script() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let profile = Self { dir };
            std::fs::create_dir_all(profile.script_cache().parent().unwrap()).unwrap();
            std::fs::write(profile.script_cache(), "cached background.js").unwrap();
            profile
        }

        fn cache(&self) -> ServiceWorkerCache {
            ServiceWorkerCache::of(self.dir.path())
        }

        fn script_cache(&self) -> PathBuf {
            self.dir
                .path()
                .join("Default")
                .join(SERVICE_WORKER_DIR)
                .join("ScriptCache")
                .join("entry_0")
        }

        fn legacy_marker(&self) -> PathBuf {
            self.dir.path().join("Default").join(LEGACY_RUNTIME_MARKER)
        }
    }

    #[test]
    fn a_profile_that_predates_the_fingerprint_loses_its_cached_scripts() {
        let profile = Profile::with_cached_script();
        std::fs::write(profile.legacy_marker(), "nngceckbapebfimnlniiiahkandclblb").unwrap();

        profile
            .cache()
            .reconcile(&[PreparedRuntime::fixture(
                "nngceckbapebfimnlniiiahkandclblb",
                "runtime-hash",
            )])
            .unwrap();

        assert!(!profile.script_cache().exists());
        assert!(!profile.legacy_marker().exists());
    }

    #[test]
    fn a_reinstalled_extension_loses_the_scripts_cached_for_its_previous_package() {
        let profile = Profile::with_cached_script();
        let cache = profile.cache();
        cache
            .reconcile(&[PreparedRuntime::fixture("bitwarden", "runtime-2026.7.0")])
            .unwrap();
        std::fs::create_dir_all(profile.script_cache().parent().unwrap()).unwrap();
        std::fs::write(profile.script_cache(), "cached background.js").unwrap();

        cache
            .reconcile(&[PreparedRuntime::fixture("bitwarden", "runtime-2026.5.1")])
            .unwrap();

        assert!(!profile.script_cache().exists());
    }

    #[test]
    fn an_unchanged_extension_set_keeps_the_cached_scripts() {
        let profile = Profile::with_cached_script();
        let cache = profile.cache();
        let prepared = [
            PreparedRuntime::fixture("bitwarden", "runtime-hash"),
            PreparedRuntime::fixture("vimium", "other-hash"),
        ];
        cache.reconcile(&prepared).unwrap();
        std::fs::create_dir_all(profile.script_cache().parent().unwrap()).unwrap();
        std::fs::write(profile.script_cache(), "cached background.js").unwrap();

        cache.reconcile(&prepared).unwrap();

        assert!(profile.script_cache().exists());
    }

    #[test]
    fn the_fingerprint_ignores_the_order_extensions_were_prepared_in() {
        let profile = Profile::with_cached_script();
        let cache = profile.cache();
        cache
            .reconcile(&[
                PreparedRuntime::fixture("bitwarden", "runtime-hash"),
                PreparedRuntime::fixture("vimium", "other-hash"),
            ])
            .unwrap();
        std::fs::create_dir_all(profile.script_cache().parent().unwrap()).unwrap();
        std::fs::write(profile.script_cache(), "cached background.js").unwrap();

        cache
            .reconcile(&[
                PreparedRuntime::fixture("vimium", "other-hash"),
                PreparedRuntime::fixture("bitwarden", "runtime-hash"),
            ])
            .unwrap();

        assert!(profile.script_cache().exists());
    }
}
