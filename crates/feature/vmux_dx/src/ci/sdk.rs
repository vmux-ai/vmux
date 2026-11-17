use super::{Arguments, Completed, Context, Io, Root, Sequence, Stage};
use crate::Settings;
use crate::ci_io::fs::{Print, Written};
use crate::ci_io::process::{Capture, Captured};
use crate::consumer_graph::Graph;
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::cli::CliInvocation;

pub(super) struct CiSdkPlugin;

impl bevy_app::Plugin for CiSdkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (start, configure_core, configure_consumer, advance, finish)
                .chain()
                .in_set(Stage::Step),
        );
    }
}

#[derive(Component)]
pub(super) struct Sdk {
    suite: String,
    commands: Vec<Capture>,
    next: usize,
}

impl Sdk {
    pub(super) const PROFILES: &'static [(&'static str, &'static str)] = &[
        ("test-sdk-ecs", "Test SDK ECS"),
        ("test-sdk-project", "Test SDK Project"),
        ("test-sdk-extension", "Test SDK Extension"),
        ("test-sdk-flex", "Test SDK Flex"),
        ("test-sdk-layout", "Test SDK Layout"),
        ("test-sdk-framework", "Test SDK Framework"),
        ("test-sdk-host", "Test SDK Host"),
    ];

    fn validate(&self, tree: &str) -> crate::Result<()> {
        match self.suite.as_str() {
            "test-sdk-framework" => Graph::check(tree),
            "test-sdk-host" => Graph::host(tree),
            _ => Ok(()),
        }
    }
}

#[derive(Component)]
struct ExecutionSlot;

#[derive(Component)]
struct PrintSlot;

#[derive(Component)]
struct Configured;

#[derive(Component)]
struct Finishing;

fn start(requests: StartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation) in &requests {
        if !invocation.is("dx.ci.sdk-test") {
            continue;
        }
        let arguments = Arguments::values(invocation);
        let [suite] = arguments else {
            io.fail(entity, "sdk-test requires one suite");
            continue;
        };
        let Some(suite) = suite.to_str() else {
            io.fail(entity, "SDK suite must be UTF-8");
            continue;
        };
        if !Sdk::PROFILES.iter().any(|(key, _)| *key == suite) {
            io.fail(entity, "Unknown SDK suite");
            continue;
        }
        io.commands.entity(entity).insert(Sdk {
            suite: suite.into(),
            commands: Vec::new(),
            next: 0,
        });
    }
}

fn configure_core(mut requests: ConfigureRequests<'_, '_>, mut io: Io) {
    for (entity, context, settings, mut sdk) in &mut requests {
        let variants: &[(&str, &str, bool)] = match sdk.suite.as_str() {
            "test-sdk-ecs" => &[
                ("vmux_ecs", "", true),
                ("vmux_ecs", "contracts", true),
                ("vmux_ecs", "store", true),
                ("vmux_ecs", "jobs", true),
            ],
            "test-sdk-project" => &[("vmux_project", "contracts,filesystem", true)],
            "test-sdk-extension" => &[("vmux_extension", "filesystem", true)],
            "test-sdk-flex" => &[("vmux_flex", "", false), ("vmux_flex", "window", true)],
            "test-sdk-layout" => &[
                ("vmux_layout", "", true),
                ("vmux_layout", "contracts,geometry", true),
                ("vmux_layout", "store", true),
            ],
            _ => continue,
        };
        let cargo = settings
            .0
            .get("CARGO_BIN")
            .and_then(|value| value.to_str())
            .unwrap_or("cargo");
        for (package, features, lib) in variants {
            let mut arguments = vec!["test", "--locked", "-p", package, "--no-default-features"];
            if !features.is_empty() {
                arguments.extend(["--features", features]);
            }
            if *lib {
                arguments.push("--lib");
            }
            sdk.commands
                .push(context.process(&settings.0, cargo, &arguments));
        }
        io.commands.entity(entity).insert(Configured);
    }
}

fn configure_consumer(mut requests: ConfigureRequests<'_, '_>, mut io: Io) {
    for (entity, context, settings, mut sdk) in &mut requests {
        let name = match sdk.suite.as_str() {
            "test-sdk-host" => "host-consumer",
            "test-sdk-framework" => "framework-consumer",
            _ => continue,
        };
        let host = sdk.suite == "test-sdk-host";
        let manifest = format!("examples/{name}/Cargo.toml");
        let target = match settings.0.get("CARGO_TARGET_DIR") {
            Some(value) => std::path::PathBuf::from(value),
            None => context.root.join("target"),
        };
        let target = target.join(name);
        let Some(target) = target.to_str() else {
            io.fail(entity, "SDK target must be UTF-8");
            continue;
        };
        let cargo = settings
            .0
            .get("CARGO_BIN")
            .and_then(|value| value.to_str())
            .unwrap_or("cargo");
        let variants: &[(&str, &str)] = if host {
            &[
                ("Vmux", "ai.vmux"),
                ("Atlas", "org.example.atlas"),
                ("Atlas", "net.example.atlas"),
            ]
        } else {
            &[("", "")]
        };
        for (name, namespace) in variants {
            let mut environment = settings.0.clone();
            if host {
                for key in settings.0.names() {
                    let Some(key) = key.to_str() else {
                        continue;
                    };
                    if key.starts_with("VMUX_HOST_") {
                        environment.remove(key);
                    }
                }
                for key in ["VMUX_DATA_DIR", "VMUX_TEST"] {
                    environment.remove(key);
                }
                for (key, value) in [
                    ("VMUX_BUILD_PROFILE", "dev"),
                    ("VMUX_PROFILE", "host-probe"),
                    ("VMUX_HOST_NAME", name),
                    ("VMUX_HOST_NAMESPACE", namespace),
                    ("EXPECTED_NAMESPACE", namespace),
                ] {
                    environment.set(key, value);
                }
            }
            sdk.commands.push(context.process(
                &environment,
                cargo,
                &[
                    "test",
                    "--locked",
                    "--manifest-path",
                    &manifest,
                    "--target-dir",
                    target,
                ],
            ));
        }
        sdk.commands.push(context.process(
            &settings.0,
            cargo,
            &[
                "tree",
                "--locked",
                "--prefix",
                "none",
                "--format",
                "{p}",
                "--manifest-path",
                &manifest,
            ],
        ));
        io.commands.entity(entity).insert(Configured);
    }
}

fn advance(
    mut requests: AdvanceRequests<'_, '_>,
    captures: super::Reads<Captured, ExecutionSlot>,
    mut io: Io,
) {
    for (entity, mut sdk, mut sequence) in &mut requests {
        if sdk.next > 0 {
            let Some(captured) = captures.get(entity) else {
                continue;
            };
            let text = match captured.text() {
                Ok(text) => text,
                Err(error) => {
                    io.fail(
                        entity,
                        format!("{error}\n{}", String::from_utf8_lossy(&captured.0.stdout)),
                    );
                    continue;
                }
            };
            if sdk.next == sdk.commands.len() {
                let result = sdk.validate(text);
                if let Err(error) = result {
                    io.fail(entity, error.to_string());
                    continue;
                }
            }
            let mut output = captured.0.stdout.clone();
            output.extend(&captured.0.stderr);
            io.emit(entity, &mut sequence, Print(output), PrintSlot);
        }
        if sdk.next == sdk.commands.len() {
            io.commands.entity(entity).insert(Finishing);
            continue;
        }
        let command = sdk.commands[sdk.next].clone();
        io.request(entity, &mut sequence, command, ExecutionSlot);
        sdk.next += 1;
    }
}

fn finish(requests: FinishRequests<'_, '_>, prints: super::Reads<Written, PrintSlot>, mut io: Io) {
    for entity in &requests {
        if prints.get(entity).is_some() {
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}

type StartRequests<'w, 's> =
    Query<'w, 's, (Entity, &'static CliInvocation), (With<Root>, Without<Sdk>, Without<Completed>)>;
type AdvanceRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut Sdk, &'static mut Sequence),
    (
        With<Root>,
        With<Configured>,
        Without<Finishing>,
        Without<Completed>,
    ),
>;
type ConfigureRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static Settings,
        &'static mut Sdk,
    ),
    (With<Root>, Without<Configured>, Without<Completed>),
>;
type FinishRequests<'w, 's> =
    Query<'w, 's, Entity, (With<Root>, With<Sdk>, With<Finishing>, Without<Completed>)>;

#[cfg(test)]
mod tests {
    use crate::ci::testing::Fixture;
    use std::os::unix::fs::PermissionsExt;

    struct SdkFixture {
        fixture: Fixture,
    }

    impl SdkFixture {
        fn new(tree: &str, failure: &str) -> Self {
            let mut fixture = Fixture::new();
            fixture.write("sdk-tree", tree);
            fixture.write("bin/cargo", format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> cargo-calls\nprintf '%s|%s|%s|%s\\n' \"${{VMUX_HOST_NAME-}}\" \"${{VMUX_HOST_NAMESPACE-}}\" \"${{VMUX_DATA_DIR-unset}}\" \"${{VMUX_TEST-unset}}\" >> environments\n{failure}\nif [ \"$1\" = tree ]; then cat sdk-tree; fi\n"));
            std::fs::set_permissions(
                fixture.root().join("bin/cargo"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
            let path = fixture.environment.get("PATH").unwrap().to_string_lossy();
            fixture.environment.set(
                "PATH",
                format!("{}:{path}", fixture.root().join("bin").display()),
            );
            Self { fixture }
        }

        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(self.fixture.root().join("cargo-calls"))
                .unwrap()
                .lines()
                .map(str::to_string)
                .collect()
        }
    }

    #[test]
    fn sdk_groups_preserve_every_minimal_feature_test_and_stop_after_a_failed_command() {
        let fixture = SdkFixture::new("", "");
        for suite in [
            "test-sdk-ecs",
            "test-sdk-project",
            "test-sdk-extension",
            "test-sdk-flex",
            "test-sdk-layout",
        ] {
            fixture.fixture.run("sdk-test", &[suite]).unwrap();
        }
        assert_eq!(
            fixture.calls(),
            [
                "test --locked -p vmux_ecs --no-default-features --lib",
                "test --locked -p vmux_ecs --no-default-features --features contracts --lib",
                "test --locked -p vmux_ecs --no-default-features --features store --lib",
                "test --locked -p vmux_ecs --no-default-features --features jobs --lib",
                "test --locked -p vmux_project --no-default-features --features contracts,filesystem --lib",
                "test --locked -p vmux_extension --no-default-features --features filesystem --lib",
                "test --locked -p vmux_flex --no-default-features",
                "test --locked -p vmux_flex --no-default-features --features window --lib",
                "test --locked -p vmux_layout --no-default-features --lib",
                "test --locked -p vmux_layout --no-default-features --features contracts,geometry --lib",
                "test --locked -p vmux_layout --no-default-features --features store --lib",
            ]
        );
        let failed = SdkFixture::new(
            "",
            "case \"$*\" in *'--features contracts'*) echo 'assertion details'; echo 'compiler failed' >&2; exit 42 ;; esac",
        );
        let error = failed
            .fixture
            .run("sdk-test", &["test-sdk-ecs"])
            .unwrap_err();
        assert!(error.contains("compiler failed"));
        assert!(error.contains("assertion details"));
        assert_eq!(failed.calls().len(), 2);
        assert!(failed.fixture.run("sdk-test", &["unknown"]).is_err());
        assert_eq!(failed.calls().len(), 2);
    }

    #[test]
    fn consumers_keep_graph_refusals_and_host_namespace_isolation() {
        let framework = "vmux_app v1\nvmux_ecs v1\nvmux_project v1\nvmux_extension v1\nvmux_flex v1\nvmux_layout v1\n";
        let fixture = SdkFixture::new(framework, "");
        fixture
            .fixture
            .run("sdk-test", &["test-sdk-framework"])
            .unwrap();
        assert_eq!(fixture.calls().len(), 2);
        for tree in [format!("{framework}bevy_cef v1\n"), "vmux_app v1\n".into()] {
            let fixture = SdkFixture::new(&tree, "");
            assert!(
                fixture
                    .fixture
                    .run("sdk-test", &["test-sdk-framework"])
                    .is_err()
            );
        }
        let mut host = SdkFixture::new(
            "vmux_app v1\nvmux_profile v1\nbevy_app v1\nbevy_ecs v1\n",
            "",
        );
        for (key, value) in [
            ("VMUX_HOST_NAME", "Inherited"),
            ("VMUX_HOST_NAMESPACE", "inherited"),
            ("VMUX_DATA_DIR", "shared-data"),
            ("VMUX_TEST", "1"),
        ] {
            host.fixture.environment.set(key, value);
        }
        host.fixture.run("sdk-test", &["test-sdk-host"]).unwrap();
        assert_eq!(host.calls().len(), 4);
        let environments =
            std::fs::read_to_string(host.fixture.root().join("environments")).unwrap();
        assert_eq!(
            environments.lines().take(3).collect::<Vec<_>>(),
            [
                "Vmux|ai.vmux|unset|unset",
                "Atlas|org.example.atlas|unset|unset",
                "Atlas|net.example.atlas|unset|unset"
            ]
        );
        let forbidden = SdkFixture::new("vmux_app v1\nvmux_profile v1\nvmux_ecs v1\n", "");
        assert!(
            forbidden
                .fixture
                .run("sdk-test", &["test-sdk-host"])
                .is_err()
        );
    }
}
