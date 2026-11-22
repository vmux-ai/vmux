use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::MutexGuard;

#[cfg(test)]
static COMPILER_QUEUE: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
struct Fixture {
    directory: tempfile::TempDir,
    library: PathBuf,
    _queue: MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Fixture {
    fn new() -> Self {
        let queue = match COMPILER_QUEUE.lock() {
            Ok(queue) => queue,
            Err(poisoned) => poisoned.into_inner(),
        };
        let directory = tempfile::tempdir().unwrap();
        let library = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        std::fs::create_dir(directory.path().join("src")).unwrap();
        std::fs::create_dir(directory.path().join("examples")).unwrap();
        std::fs::write(directory.path().join("src/lib.rs"), "").unwrap();
        std::fs::copy(
            library.join("Cargo.toml"),
            directory.path().join("Cargo.toml"),
        )
        .unwrap();
        std::fs::copy(
            library.join("Cargo.lock"),
            directory.path().join("Cargo.lock"),
        )
        .unwrap();
        Self {
            directory,
            library,
            _queue: queue,
        }
    }

    fn check(&self, name: &str, source: &str, expected: &[&str], code: &str) {
        std::fs::write(
            self.directory
                .path()
                .join("examples")
                .join(format!("{name}.rs")),
            source,
        )
        .unwrap();
        let output = Command::new("cargo")
            .env("DYLINT_RUSTFLAGS", "-A vmux_ecs_runtime")
            .current_dir(self.directory.path())
            .args([
                "dylint",
                "--path",
                self.library.to_str().unwrap(),
                "--",
                "--locked",
                "--example",
                name,
                "--message-format=json",
            ])
            .output()
            .expect("run the prepared Dylint compiler");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        let (mut diagnostics, unexpected) = self.diagnostics(name, &stdout, code);
        assert!(unexpected.is_empty(), "{name}: {unexpected:?}\n{stderr}");
        let mut expected: Vec<String> = expected
            .iter()
            .map(|name| {
                if code == "vmux_exported_system" {
                    format!("registered system `{name}` must be private to its defining module")
                } else {
                    name.to_string()
                }
            })
            .collect();
        expected.sort();
        diagnostics.sort();
        assert_eq!(diagnostics, expected, "{name}: {stderr}");
        assert_eq!(
            output.status.success(),
            expected.is_empty(),
            "{name}: {stderr}"
        );
    }

    fn diagnostics(&self, name: &str, stdout: &str, code: &str) -> (Vec<String>, Vec<String>) {
        let mut diagnostics = Vec::new();
        let mut unexpected = Vec::new();
        for line in stdout.lines() {
            let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if event["reason"] != "compiler-message" {
                continue;
            }
            let message = &event["message"];
            if message["level"] != "error"
                && !(message["level"] == "warning"
                    && message["code"]["code"] == "vmux_ecs_ownership")
            {
                continue;
            }
            if message["code"]["code"] != code {
                unexpected.push(message.to_string());
                continue;
            }
            let spans = message["spans"]
                .as_array()
                .expect("source-located lint diagnostic");
            assert!(
                spans.iter().any(|span| {
                    span["is_primary"] == true
                        && Path::new(span["file_name"].as_str().unwrap())
                            .file_stem()
                            .unwrap()
                            == name
                        && span["line_start"].as_u64().unwrap() > 0
                }),
                "{name}: {message}"
            );
            let mut diagnostic = message["message"].as_str().unwrap().to_owned();
            if code == "vmux_ecs_ownership" {
                let primary = spans
                    .iter()
                    .find(|span| span["is_primary"] == true)
                    .unwrap();
                diagnostic.push_str(&format!(
                    " @ {}:{}-{}:{}",
                    primary["line_start"],
                    primary["column_start"],
                    primary["line_end"],
                    primary["column_end"]
                ));
            }
            diagnostics.push(diagnostic);
        }
        (diagnostics, unexpected)
    }
}

#[test]
fn runtime_inputs_follow_resolved_systems_and_system_param_methods() {
    let fixture = Fixture::new();
    for (name, source, expected) in [
        (
            "runtime_inputs",
            include_str!("../ui/runtime_inputs.rs"),
            &[
                "`inspect`: inherent methods with ECS runtime inputs must belong to a SystemParam",
                "`inspect_access`: inherent methods with ECS runtime inputs must belong to a SystemParam",
                "`isolated`: ECS runtime helpers must be private and have one local registered owner",
                "`shared`: ECS runtime helpers must be private and have one local registered owner",
                "`exposed`: ECS runtime helpers must be private and have one local registered owner",
                "`detached`: ECS runtime helpers must be private and have one local registered owner",
                "`unregistered`: ECS runtime helpers must be private and have one local registered owner",
                "`generic`: ECS runtime helpers must be private and have one local registered owner",
            ][..],
        ),
        (
            "runtime_inputs_valid",
            include_str!("../ui/runtime_inputs_valid.rs"),
            &[][..],
        ),
    ] {
        fixture.check(name, source, expected, "vmux_ecs_runtime");
    }
}

#[test]
fn compiler_scope_includes_owned_consumer_packages_and_preserves_external_packages() {
    let fixture = Fixture::new();
    let manifest = fixture.directory.path().join("Cargo.toml");
    let package = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, package.replace("vmux_lints", "consumer")).unwrap();
    fixture.check(
        "external_consumer",
        include_str!("../ui/plugin_names.rs"),
        &[],
        "vmux_plugin_name",
    );
    std::fs::create_dir(fixture.directory.path().join("crates")).unwrap();
    fixture.check(
        "owned_consumer",
        include_str!("../ui/plugin_names.rs"),
        &[
            "Bevy plugin `Http` must use a descriptive name ending in `Plugin`",
            "Bevy plugin `Alias` must use a descriptive name ending in `Plugin`",
            "Bevy plugin `Plugin` must use a descriptive name ending in `Plugin`",
            "Bevy plugin `Engine` must use a descriptive name ending in `Plugin`",
            "Bevy plugin `Root` must use a descriptive name ending in `Plugin`",
            "Bevy plugin `Root` must use a descriptive name ending in `Plugin`",
        ],
        "vmux_plugin_name",
    );
}

#[test]
fn plugin_names_follow_real_implementations_aliases_and_globs() {
    let fixture = Fixture::new();
    for (name, source, expected) in [
        (
            "plugin_names",
            include_str!("../ui/plugin_names.rs"),
            &[
                "Bevy plugin `Http` must use a descriptive name ending in `Plugin`",
                "Bevy plugin `Alias` must use a descriptive name ending in `Plugin`",
                "Bevy plugin `Plugin` must use a descriptive name ending in `Plugin`",
                "Bevy plugin `Engine` must use a descriptive name ending in `Plugin`",
                "Bevy plugin `Root` must use a descriptive name ending in `Plugin`",
                "Bevy plugin `Root` must use a descriptive name ending in `Plugin`",
            ][..],
        ),
        (
            "plugin_names_valid",
            include_str!("../ui/plugin_names_valid.rs"),
            &[][..],
        ),
    ] {
        fixture.check(name, source, expected, "vmux_plugin_name");
    }
}

#[test]
fn registered_system_names_follow_resolved_functions_and_ignore_other_methods() {
    let fixture = Fixture::new();
    for (name, source, expected) in [
        (
            "system_names",
            include_str!("../ui/system_names.rs"),
            &[
                "registered system `bookmark_pin` must use its module's operation name: remove repeated `bookmark` context",
                "registered system `pin_system` must use its module's operation name: remove `system`",
                "registered system `bookmark_condition` must use its module's operation name: remove repeated `bookmark` context",
                "registered system `bookmark_observe` must use its module's operation name: remove repeated `bookmark` context",
                "registered system `bookmark_once` must use its module's operation name: remove repeated `bookmark` context",
                "registered system `bookmark_bind` must use its module's operation name: remove repeated `bookmark` context",
                "registered system `bookmark_remove` must use its module's operation name: remove repeated `bookmark` context",
                "registered system `lints_sync` must use its module's operation name: remove repeated `lints` context",
            ][..],
        ),
        (
            "system_names_valid",
            include_str!("../ui/system_names_valid.rs"),
            &[][..],
        ),
    ] {
        fixture.check(name, source, expected, "vmux_system_name");
    }
}

#[test]
fn compiler_resolves_observers_and_ignores_lookalike_registration_apis() {
    let fixture = Fixture::new();
    for (name, source, expected) in [
        (
            "observers",
            include_str!("../ui/observers.rs"),
            &[
                "public",
                "crate_visible",
                "parent_visible",
                "world_visible",
                "entity_visible",
                "commands_visible",
                "deferred_visible",
                "constructed",
            ][..],
        ),
        (
            "unrelated_observers",
            include_str!("../ui/unrelated_observers.rs"),
            &[][..],
        ),
        (
            "runtime_callbacks",
            include_str!("../ui/runtime_callbacks.rs"),
            &[
                "added",
                "removed",
                "once",
                "explicit",
                "tail",
                "labeled",
                "fallible_added",
                "fallible_inserted",
                "fallible_discarded",
                "fallible_removed",
                "fallible_despawned",
            ][..],
        ),
    ] {
        fixture.check(name, source, expected, "vmux_exported_system");
    }
}

#[test]
fn compiler_resolves_registered_systems_and_ignores_other_contracts() {
    let fixture = Fixture::new();
    for (name, source, expected) in [
        ("private", include_str!("../ui/private.rs"), &[][..]),
        ("unrelated", include_str!("../ui/unrelated.rs"), &[][..]),
        (
            "exported",
            include_str!("../ui/exported.rs"),
            &["public", "crate_visible", "parent_visible"][..],
        ),
        (
            "aliases",
            include_str!("../ui/aliases.rs"),
            &["exported"][..],
        ),
        (
            "adapters",
            include_str!("../ui/adapters.rs"),
            &["source", "sink", "mapped"][..],
        ),
        (
            "qualified",
            include_str!("../ui/qualified.rs"),
            &["exported"][..],
        ),
        (
            "reassigned",
            include_str!("../ui/reassigned.rs"),
            &["exported"][..],
        ),
        ("overwritten", include_str!("../ui/overwritten.rs"), &[][..]),
        (
            "inside_branch",
            include_str!("../ui/inside_branch.rs"),
            &[][..],
        ),
        ("branch_join", include_str!("../ui/branch_join.rs"), &[][..]),
        (
            "uninitialized",
            include_str!("../ui/uninitialized.rs"),
            &["exported"][..],
        ),
        (
            "block_results",
            include_str!("../ui/block_results.rs"),
            &["exported_block", "exported_if", "exported_match"][..],
        ),
        (
            "loops",
            include_str!("../ui/loops.rs"),
            &["carried", "continued", "from_loop"][..],
        ),
    ] {
        fixture.check(name, source, expected, "vmux_exported_system");
    }
}

#[test]
fn ownership_diagnostics_require_an_explicit_audit_request() {
    let fixture = Fixture::new();
    fixture.check(
        "ownership_default",
        "struct Detached(u32); fn main() {}\n",
        &[],
        "vmux_ecs_ownership",
    );
}

#[test]
fn compiler_proves_registered_ownership_and_rejects_hidden_or_unused_models() {
    let fixture = Fixture::new();
    for (name, source, expected) in [
        ("owned", include_str!("../ui/owned.rs"), &[][..]),
        (
            "shared",
            include_str!("../ui/shared.rs"),
            &[
                "declaration has no unique resolved registered ECS owner @ 7:1-7:14",
                "canonical ECS state hides a dynamic engine @ 21:5-21:20",
            ][..],
        ),
        ("projected", include_str!("../ui/projected.rs"), &[][..]),
        (
            "reachability",
            include_str!("../ui/reachability.rs"),
            &["declaration has no unique resolved registered ECS owner @ 27:5-27:20"][..],
        ),
        (
            "value_graph",
            include_str!("../ui/value_graph.rs"),
            &[
                "canonical ECS state embeds an independently owned model @ 12:5-12:23",
                "canonical ECS state embeds an independently owned model @ 17:5-17:29",
                "canonical ECS state retains a recursive model @ 22:5-22:33",
                "declaration has no unique resolved registered ECS owner @ 7:1-7:11",
            ][..],
        ),
        (
            "unused",
            include_str!("../ui/unused.rs"),
            &[
                "declaration has no unique resolved registered ECS owner @ 7:1-7:14",
                "declaration has no unique resolved registered ECS owner @ 10:1-10:20",
                "declaration has no unique resolved registered ECS owner @ 18:5-18:18",
            ][..],
        ),
        (
            "hidden",
            include_str!("../ui/hidden.rs"),
            &[
                "canonical ECS state embeds an independently owned model @ 12:5-12:19",
                "canonical ECS state embeds an independently owned model @ 22:5-22:31",
                "canonical ECS state embeds an independently owned model @ 37:5-37:17",
                "canonical ECS state hides a dynamic engine @ 29:5-29:28",
                "declaration has no unique resolved registered ECS owner @ 11:1-11:13",
                "declaration has no unique resolved registered ECS owner @ 7:1-7:11",
            ][..],
        ),
        (
            "filtered",
            include_str!("../ui/filtered.rs"),
            &[
                "declaration has no unique resolved registered ECS owner @ 10:1-10:15",
                "declaration has no unique resolved registered ECS owner @ 8:1-8:18",
                "registered system retains Local state without a canonical ECS owner @ 17:1-17:59",
            ][..],
        ),
    ] {
        fixture.check(name, source, expected, "vmux_ecs_ownership");
    }
}
