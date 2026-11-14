#![cfg(unix)]

use std::fs;

use support::Project;

pub mod support;

#[test]
fn compiled_and_packaged_titles_keep_the_product_name_and_source_label() {
    let project = Project::create();
    fs::copy(
        project.repo().join("crates/build_git_env.rs"),
        project.root().join("build_git_env.rs"),
    )
    .unwrap();
    project.script(
        "check-titles.py",
        r#"import os,pathlib,plistlib,subprocess
root=pathlib.Path.cwd()
for key in ['GIT_DIR','GIT_WORK_TREE','GIT_INDEX_FILE','GIT_COMMON_DIR']:
    os.environ.pop(key,None)
subprocess.run(['/usr/bin/git','init','--initial-branch=title-fixture'],check=True,capture_output=True)
(root/'title.rs').write_text('mod build_git_env;\nfn main() { build_git_env::emit(); }\n')
subprocess.run(['rustc','--edition=2024','title.rs','-o','title-emit'],check=True)
for name in [None,'Atlas "Lab" & Co']:
    for profile in ['release','dev','local','review']:
        env=os.environ.copy()
        env['VMUX_BUILD_PROFILE']=profile
        if name is None:
            env.pop('VMUX_HOST_NAME',None)
        else:
            env['VMUX_HOST_NAME']=name
        product=name or 'Vmux'
        expected=product
        if profile!='release':
            expected+=(' Dev' if profile=='dev' else '')+' ('+('title-fixture' if profile in ['dev','local'] else profile)+')'
        output=subprocess.check_output([str(root/'title-emit')],env=env,text=True)
        prefix='cargo::rustc-env=VMUX_BUILD_TITLE='
        compiled=next(line[len(prefix):] for line in output.splitlines() if line.startswith(prefix))
        plist=root/'title.plist'
        plist.write_bytes(plistlib.dumps({'Unrelated':'retained'}))
        packaged=subprocess.check_output(['bash','scripts/build-title.sh',profile,str(root),str(plist)],env=env,text=True).strip()
        info=plistlib.loads(plist.read_bytes())
        assert compiled==packaged==expected,(name,profile,compiled,packaged,expected)
        assert info['CFBundleName']==info['CFBundleDisplayName']==expected
        assert info['Unrelated']=='retained'
        assert 'cargo::rerun-if-env-changed=VMUX_HOST_NAME' in output.splitlines()
"#,
    );
    let output = project
        .command("python3")
        .arg("check-titles.py")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn host_metadata_round_trips_shell_and_json_metacharacters_without_execution() {
    let project = Project::create();
    project.script(
        "check-host.sh",
        r#"#!/usr/bin/env bash
set -euo pipefail
source scripts/product-host.sh
vmux_host_read "$PWD"
python3 - <<'PY'
import os
assert os.environ['VMUX_HOST_NAME'] == os.environ['EXPECTED_NAME']
assert os.environ['VMUX_HOST_KEYCHAIN_SERVICE'] == 'org.example.atlas.safe-storage'
assert os.environ['VMUX_HOST_CONFIG_NAME'] != '.vmux'
assert os.environ['VMUX_HOST_STABLE_ENDPOINT'] == ''
assert os.environ['VMUX_HOST_CAMERA'] == 'false'
PY
"#,
    );
    let name = "Atlas \"Lab\" & $(touch unexpected)";
    let output = project
        .command("./check-host.sh")
        .env("VMUX_HOST_NAME", name)
        .env("VMUX_HOST_NAMESPACE", "org.example.atlas")
        .env("EXPECTED_NAME", name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!project.root().join("unexpected").exists());
}

#[test]
fn invalid_host_identity_is_refused_before_building_or_marking_ready() {
    for (key, value) in [
        ("VMUX_HOST_NAME", "Other"),
        ("VMUX_HOST_NAMESPACE", "org.example.other"),
        ("VMUX_HOST_DATA_NAME", "../Vmux"),
        ("VMUX_HOST_CAMERA", "yes"),
        ("VMUX_HOST_ICON", "missing-icon.icns"),
        ("VMUX_HOST_STABLE_ENDPOINT", "https://"),
        ("VMUX_HOST_STABLE_ENDPOINT", "https:///updates.json"),
        ("VMUX_HOST_STABLE_ENDPOINT", "https://?channel=stable"),
        (
            "VMUX_HOST_STABLE_ENDPOINT",
            "https://user@example.com/updates.json",
        ),
        (
            "VMUX_HOST_STABLE_ENDPOINT",
            "https://example.com\\other/updates.json",
        ),
        (
            "VMUX_HOST_STABLE_ENDPOINT",
            "https://example.com:invalid/updates.json",
        ),
        (
            "VMUX_HOST_STABLE_ENDPOINT",
            "https://example.com:65536/updates.json",
        ),
        (
            "VMUX_HOST_STABLE_ENDPOINT",
            "https://[invalid]/updates.json",
        ),
        (
            "VMUX_HOST_STABLE_ENDPOINT",
            "https://127.0.0.999/updates.json",
        ),
        (
            "VMUX_HOST_STABLE_ENDPOINT",
            "https://example.com/\"updates.json",
        ),
        ("VMUX_HOST_STABLE_ENDPOINT", "https://example.com/%zz"),
        (
            "VMUX_HOST_STABLE_ENDPOINT",
            "https://example.com/updates.json#other",
        ),
        (
            "VMUX_HOST_STABLE_ENDPOINT",
            "https://example.com../updates.json",
        ),
    ] {
        let project = Project::create();
        let metadata = project
            .command(env!("CARGO_BIN_EXE_vmux_builder"))
            .arg("--root")
            .arg(project.root())
            .arg("host-config")
            .env(key, value)
            .output()
            .unwrap();
        assert!(!metadata.status.success(), "accepted {key}={value}");
        if key == "VMUX_HOST_STABLE_ENDPOINT" {
            assert!(
                String::from_utf8_lossy(&metadata.stderr).contains("HTTPS URL"),
                "{}",
                String::from_utf8_lossy(&metadata.stderr)
            );
        }
        project.write_target("debug/.vmux-dev-ready", "previous build");
        let output = project
            .command(env!("CARGO_BIN_EXE_vmux_builder"))
            .arg("--root")
            .arg(project.root())
            .arg("prepare-dev")
            .env(key, value)
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted {key}={value}");
        assert!(!project.root().join("build.log").exists());
        assert!(!project.target().join("debug/.vmux-dev-ready").exists());
    }
}

#[test]
fn custom_namespaces_isolate_storage_and_refuse_case_insensitive_default_aliases() {
    let project = Project::create();
    for (key, value) in [
        ("VMUX_HOST_NAME", "vmux"),
        ("VMUX_HOST_DATA_NAME", "vMuX"),
        ("VMUX_HOST_CONFIG_NAME", ".VMUX"),
    ] {
        let output = project
            .command(env!("CARGO_BIN_EXE_vmux_builder"))
            .arg("--root")
            .arg(project.root())
            .arg("host-config")
            .env("VMUX_HOST_NAME", "Atlas")
            .env("VMUX_HOST_NAMESPACE", "org.example.atlas")
            .env(key, value)
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted {key}={value}");
    }
    let mut outputs = Vec::new();
    for namespace in ["org.example.atlas", "net.example.atlas"] {
        let output = project
            .command(env!("CARGO_BIN_EXE_vmux_builder"))
            .arg("--root")
            .arg(project.root())
            .arg("host-config")
            .env("VMUX_HOST_NAME", "Atlas")
            .env("VMUX_HOST_NAMESPACE", namespace)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        outputs.push(String::from_utf8(output.stdout).unwrap());
    }
    let output = project.command("python3").arg("-c").arg(r#"
import json,sys
a,b=map(json.loads,sys.argv[1:3])
for key in ['VMUX_HOST_DATA_NAME','VMUX_HOST_CONFIG_NAME','VMUX_HOST_RUNTIME_STEM','VMUX_HOST_KEYCHAIN_SERVICE','VMUX_HOST_SERVICE_ID']:
    assert a[key]!=b[key], key
"#).args(outputs).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn update_endpoints_preserve_valid_authorities_and_encoded_paths() {
    let project = Project::create();
    for endpoint in [
        "",
        "https://updates.example.com/releases/updates.json?channel=stable",
        "https://127.0.0.1:8443/updates.json",
        "https://[::1]:8443/updates.json",
        "https://xn--bcher-kva.example/a%20b.json",
    ] {
        let output = project
            .command(env!("CARGO_BIN_EXE_vmux_builder"))
            .arg("--root")
            .arg(project.root())
            .arg("host-config")
            .env("VMUX_HOST_STABLE_ENDPOINT", endpoint)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{endpoint}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let decoded = project.command("python3")
            .arg("-c")
            .arg("import json,sys; assert json.loads(sys.argv[1])['VMUX_HOST_STABLE_ENDPOINT'] == sys.argv[2]")
            .arg(String::from_utf8(output.stdout).unwrap())
            .arg(endpoint)
            .output()
            .unwrap();
        assert!(
            decoded.status.success(),
            "{}",
            String::from_utf8_lossy(&decoded.stderr)
        );
    }
}

#[test]
fn custom_service_plists_and_executable_use_one_escaped_product_identity() {
    let project = Project::create();
    for file in [
        "scripts/embed-launch-agent-plist.sh",
        "packaging/macos/ai.vmux.service.plist",
    ] {
        let target = project.root().join(file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(project.repo().join(file), target).unwrap();
    }
    let bundle = project.root().join("Atlas.app");
    fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    fs::write(
        bundle.join("Contents/MacOS/Atlas & Co Service"),
        "fixture service",
    )
    .unwrap();
    let output = project
        .command("bash")
        .arg("scripts/embed-launch-agent-plist.sh")
        .env("VMUX_HOST_NAME", "Atlas & Co")
        .env("VMUX_HOST_NAMESPACE", "org.example.atlas")
        .env("VMUX_APP_BUNDLE", &bundle)
        .env("VMUX_BUILD_PROFILE", "local")
        .env("VMUX_GIT_HASH", "abc1234")
        .env("VMUX_BUNDLE_ID", "org.example.atlas.desktop.abc1234")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = project.command("python3").arg("-c").arg(r#"
import pathlib,plistlib,sys
bundle=pathlib.Path(sys.argv[1])
service=bundle/'Contents/Library/LoginItems/Atlas & Co Service.app/Contents'
assert (service/'MacOS/Atlas & Co Service').read_text() == 'fixture service'
with (service/'Info.plist').open('rb') as source: info=plistlib.load(source)
assert info['CFBundleIdentifier']=='org.example.atlas.service.abc1234'
assert info['CFBundleDisplayName']=='Atlas & Co Service'
assert (service/'Resources'/(info['CFBundleIconFile']+'.icns')).read_bytes()==pathlib.Path(sys.argv[2]).read_bytes()
with (bundle/'Contents/Library/LaunchAgents/org.example.atlas.service.plist').open('rb') as source: launch=plistlib.load(source)
assert launch['Label']==info['CFBundleIdentifier']
assert (bundle/launch['BundleProgram']).is_file()
assert launch['AssociatedBundleIdentifiers']==['org.example.atlas.desktop.abc1234']
assert launch['StandardOutPath']=='/tmp/org-example-atlas-service.log'
"#).arg(&bundle).arg(project.root().join("packaging/macos/Vmux.icns")).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fast_packaging_refuses_release_local_and_unverified_branches_before_mutating_templates() {
    for (profile, event, branch, preflight, release) in [
        ("local", "pull_request", "host-config", "true", "0"),
        ("release", "pull_request", "host-config", "true", "1"),
        ("release", "push", "host-config", "false", "0"),
        ("release", "push", "main", "true", "0"),
        ("release", "push", "master", "true", "0"),
        ("release", "push", "release/next", "true", "0"),
        ("release", "workflow_dispatch", "host-config", "true", "0"),
    ] {
        let project = Project::create();
        for file in [
            "crates/app/vmux_desktop/Cargo.toml",
            "packaging/macos/Info.plist",
            "scripts/package.sh",
        ] {
            let target = project.root().join(file);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(project.repo().join(file), target).unwrap();
        }
        project.script(
            "scripts/cargo-with-cef-cache.sh",
            "#!/bin/sh\ntouch packager-called\nexit 99\n",
        );
        let cargo = project.root().join("crates/app/vmux_desktop/Cargo.toml");
        let plist = project.root().join("packaging/macos/Info.plist");
        let original_cargo = fs::read(&cargo).unwrap();
        let original_plist = fs::read(&plist).unwrap();
        let output = project
            .command("bash")
            .arg("scripts/package.sh")
            .arg(profile)
            .env("VMUX_HOST_NAME", "Atlas")
            .env("VMUX_HOST_NAMESPACE", "org.example.atlas")
            .env("VMUX_CI_FAST_PACKAGE", "1")
            .env("GITHUB_EVENT_NAME", event)
            .env("GITHUB_REF", format!("refs/heads/{branch}"))
            .env("VMUX_CI_PREFLIGHT", preflight)
            .env("IS_RELEASE_PR", release)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(!project.root().join("packager-called").exists());
        assert_eq!(fs::read(&cargo).unwrap(), original_cargo);
        assert_eq!(fs::read(&plist).unwrap(), original_plist);
    }
}

#[test]
fn packaging_projects_selected_assets_and_identity_and_restores_templates_on_failure() {
    for custom in [false, true] {
        for (event, fail) in [
            ("local", false),
            ("local", true),
            ("pull_request", false),
            ("pull_request", true),
            ("push", false),
            ("push", true),
        ] {
            let project = Project::create();
            for file in [
                "Cargo.toml",
                "crates/app/vmux_desktop/Cargo.toml",
                "packaging/macos/Info.plist",
                "packaging/macos/ai.vmux.service.plist",
                "scripts/package.sh",
                "scripts/before-each-package.sh",
                "scripts/embed-launch-agent-plist.sh",
                "scripts/sign-and-notarize.sh",
            ] {
                let target = project.root().join(file);
                fs::create_dir_all(target.parent().unwrap()).unwrap();
                fs::copy(project.repo().join(file), target).unwrap();
            }
            project.script("scripts/inject-cef.sh", "#!/bin/sh\nexit 0\n");
            project.script("bin/file", "#!/bin/sh\nprintf 'Mach-O\\n'\n");
            project.script(
                "bin/codesign",
                r#"#!/usr/bin/env python3
import json,os,pathlib,sys
with open(os.environ['TEST_SIGN_LOG'],'a') as log:
    log.write(json.dumps(sys.argv[1:])+'\n')
if '--display' in sys.argv:
    pathlib.Path('codesign0').write_text('certificate')
"#,
            );

            project.script(
                "scripts/cargo-with-cef-cache.sh",
                r#"#!/bin/bash
set -euo pipefail
CARGO_PACKAGER_FORMAT=app bash scripts/before-each-package.sh
python3 - "$@" <<'PY'
import json,os,pathlib,plistlib,tomllib,shutil,sys
root=pathlib.Path.cwd()
with (root/'crates/app/vmux_desktop/Cargo.toml').open('rb') as source:
    metadata=tomllib.load(source)['package']['metadata']['packager']
name=os.environ['EXPECTED_NAME']
namespace=os.environ['EXPECTED_NAMESPACE']
suffix=os.environ['EXPECTED_SUFFIX']
assert metadata['product-name']==name+suffix
assert metadata['identifier']==namespace+'.desktop'+('.9999999' if suffix else '')
if not suffix:
    assert sys.argv[1:]==['packager','--release','--formats','app'],sys.argv
assert pathlib.Path(metadata['icons'][0]).is_file()
assert pathlib.Path(metadata['macos']['entitlements']).is_file()
assert pathlib.Path(metadata['macos']['info-plist-path']).is_file()
assert metadata['binaries'][-1]['path']==name+' Service'
if os.environ['FAIL_PACKAGE']=='1':
    raise SystemExit(42)
bundle=pathlib.Path(os.environ['VMUX_APP_BUNDLE'])
if bundle.exists():
    shutil.rmtree(bundle)
contents=bundle/'Contents'
(contents/'MacOS').mkdir(parents=True)
(contents/'Frameworks').mkdir()
(contents/'Frameworks/lib-fixture.dylib').write_text('non-executable dylib')
for executable in ['vmux_desktop','vmux',name+' Service']:
    binary=contents/'MacOS'/executable
    binary.write_text('fixture binary')
    binary.chmod(0o700)
(contents/'Info.plist').write_bytes((root/'packaging/macos/Info.plist').read_bytes())
PY
"#,
            );
            let name = if custom { "Atlas \"Lab\" & Co" } else { "Vmux" };
            let namespace = if custom {
                "org.example.atlas"
            } else {
                "ai.vmux"
            };
            let cargo = project.root().join("crates/app/vmux_desktop/Cargo.toml");
            let plist = project.root().join("packaging/macos/Info.plist");
            let original_cargo = fs::read(&cargo).unwrap();
            let original_plist = fs::read(&plist).unwrap();
            let suffix = if event == "local" { " (9999999)" } else { "" };
            let mut command = if event == "local" {
                let mut command = project.command(env!("CARGO_BIN_EXE_vmux_builder"));
                command
                    .arg("--root")
                    .arg(project.root())
                    .arg("prepare-local");
                command
            } else {
                let mut command = project.command("bash");
                command
                    .arg("scripts/package.sh")
                    .arg("release")
                    .env("VMUX_CI_FAST_PACKAGE", "1")
                    .env("GITHUB_EVENT_NAME", event)
                    .env("GITHUB_REF", "refs/heads/host-config")
                    .env("VMUX_CI_PREFLIGHT", "true")
                    .env("IS_RELEASE_PR", "0");
                command
            };
            let output = command
                .env("VMUX_HOST_NAME", name)
                .env("VMUX_HOST_NAMESPACE", namespace)
                .env("EXPECTED_NAME", name)
                .env("EXPECTED_NAMESPACE", namespace)
                .env("EXPECTED_SUFFIX", suffix)
                .env("FAIL_PACKAGE", if fail { "1" } else { "0" })
                .env("SKIP_NOTARIZE", "1")
                .env("TEST_SIGN_LOG", project.root().join("sign.jsonl"))
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                !fail,
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(fs::read(&cargo).unwrap(), original_cargo);
            assert_eq!(fs::read(&plist).unwrap(), original_plist);
            let bundle = project.target().join(format!("release/{name}{suffix}.app"));
            if fail {
                assert!(!bundle.exists());
                continue;
            }
            assert_eq!(
                fs::read(bundle.join("Contents/Resources/tray.png")).unwrap(),
                fs::read(project.root().join("packaging/macos/tray.png")).unwrap()
            );
            let output = project.command("python3").arg("-c").arg(r#"
import pathlib,plistlib,sys
bundle=pathlib.Path(sys.argv[1]); name=sys.argv[2]; namespace=sys.argv[3]; suffix=sys.argv[5]
identity_suffix='.9999999' if suffix else ''
info=plistlib.load((bundle/'Contents/Info.plist').open('rb'))
assert info['CFBundleDisplayName']==name+suffix,(info['CFBundleDisplayName'],name+suffix)
assert info['CFBundleIdentifier']==namespace+'.desktop'+identity_suffix
agent=plistlib.load((bundle/'Contents/Library/LaunchAgents'/(namespace+'.service.plist')).open('rb'))
assert agent['Label']==namespace+'.service'+identity_suffix
assert (bundle/agent['BundleProgram']).read_text()=='fixture binary'
assert agent['AssociatedBundleIdentifiers']==[info['CFBundleIdentifier']]
signs=[__import__('json').loads(line) for line in pathlib.Path(sys.argv[4]).read_text().splitlines()]
assert any('--identifier' in args and namespace+'.cli' in args and str(bundle/'Contents/MacOS/vmux') in args for args in signs)
assert any(str(bundle/'Contents/Frameworks/lib-fixture.dylib') in args for args in signs)
assert any(str(bundle/'Contents/Library/LoginItems'/(name+' Service.app')) in args for args in signs)
"#).arg(bundle).arg(name).arg(namespace).arg(project.root().join("sign.jsonl")).arg(suffix).output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
