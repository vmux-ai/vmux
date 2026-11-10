#!/usr/bin/env python3
import fcntl
import importlib.util
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('build_storage', Path(__file__).with_name('build-storage.py'))
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)


class Fixture:
    def __init__(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='vmux storage ')
        base = Path(self.temporary.name).resolve()
        self.root = base / 'checkout with spaces'
        self.target = base / 'target with spaces'
        self.root.mkdir()
        self.target.mkdir()
        self.policy = module.Policy(20 * 1024**3, 50 * 1024**3, 50 * 1024**3, 3)
        self.storage = module.Storage(self.root, self.target, self.policy)
        self.idle = module.Processes([], [])
        self.write('CACHEDIR.TAG', 'Signature: 8a477f597d28d172789f06886806bc55')

    def write(self, relative, value='fixture'):
        path = self.target / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value)
        return path

    def caches(self, profile):
        self.write(profile + '/.fingerprint/current')
        self.write(profile + '/deps/object.o', 'cached object' * 1024)
        self.write(profile + '/deps/library.rlib')
        self.write(profile + '/deps/current.dylib')
        self.write(profile + '/deps/current.so')
        self.write(profile + '/build/out/generated')
        self.write(profile + '/incremental/state')
        self.write(profile + '/vmux_desktop', 'ready executable')
        self.write(profile + '/Vmux Dev.app/Contents/MacOS/vmux_desktop', 'signed application')
        self.write('.vmux-seed/compatible')

    def app(self, revision, timestamp):
        name = f'release/Vmux ({revision}).app'
        self.write(name + '/Contents/MacOS/vmux_desktop', 'signed application')
        path = self.target / name
        os.utime(path, (timestamp, timestamp))
        return path

    def close(self):
        self.temporary.cleanup()


class StorageTests(unittest.TestCase):
    def setUp(self):
        self.fixture = Fixture()
        self.addCleanup(self.fixture.close)

    def test_cache_pruning_keeps_prepared_binaries_and_runtime_libraries(self):
        fixture = self.fixture
        for profile in ['debug', 'release', 'cef-helper', 'aarch64-apple-ios/debug']:
            fixture.caches(profile)
        fixture.storage.prune_caches(fixture.idle)
        for profile in ['debug', 'release', 'cef-helper', 'aarch64-apple-ios/debug']:
            base = fixture.target / profile
            self.assertEqual((base / 'vmux_desktop').read_text(), 'ready executable')
            self.assertEqual((base / 'Vmux Dev.app/Contents/MacOS/vmux_desktop').read_text(), 'signed application')
            self.assertTrue((base / 'deps/current.dylib').is_file())
            self.assertTrue((base / 'deps/current.so').is_file())
            self.assertFalse((base / 'deps/object.o').exists())
            self.assertFalse((base / 'deps/library.rlib').exists())
            self.assertFalse((base / '.fingerprint').exists())
        self.assertFalse((fixture.target / '.vmux-seed').exists())

    def test_active_profile_is_not_pruned(self):
        fixture = self.fixture
        fixture.caches('debug')
        fixture.caches('release')
        active = module.Processes([], [str(fixture.target / 'debug/Vmux Dev.app/Contents/MacOS/vmux_desktop')])
        fixture.storage.prune_caches(active)
        self.assertTrue((fixture.target / 'debug/deps/object.o').is_file())
        self.assertFalse((fixture.target / 'release/deps/object.o').exists())

    def test_raw_cargo_lock_is_respected(self):
        fixture = self.fixture
        fixture.caches('debug')
        with open(fixture.target / 'debug/.cargo-lock', 'w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            fixture.storage.prune_caches(fixture.idle)
        self.assertTrue((fixture.target / 'debug/deps/object.o').exists())
        self.assertTrue((fixture.target / '.vmux-seed/compatible').exists())

    def test_unknown_target_and_external_symlink_are_not_deleted(self):
        fixture = self.fixture
        fixture.caches('debug')
        (fixture.target / 'CACHEDIR.TAG').unlink()
        fixture.storage.prune_caches(fixture.idle)
        self.assertTrue((fixture.target / 'debug/deps/object.o').exists())
        fixture.write('CACHEDIR.TAG', 'Signature: 8a477f597d28d172789f06886806bc55')
        outside = fixture.root / 'user-data'
        outside.mkdir()
        (outside / 'important').write_text('user data')
        (fixture.target / 'debug/examples').symlink_to(outside, target_is_directory=True)
        fixture.storage.prune_caches(fixture.idle)
        self.assertEqual((outside / 'important').read_text(), 'user data')
        self.assertTrue((fixture.target / 'debug/examples').is_symlink())

    def test_dry_run_leaves_cache_and_compatibility_marker(self):
        fixture = self.fixture
        fixture.caches('debug')
        fixture.storage.prune_caches(fixture.idle, apply=False)
        self.assertTrue((fixture.target / 'debug/deps/object.o').exists())
        self.assertTrue((fixture.target / 'debug/.fingerprint/current').exists())
        self.assertTrue((fixture.target / '.vmux-seed/compatible').exists())

    def test_app_retention_keeps_current_old_mtime_and_running_app(self):
        fixture = self.fixture
        apps = [fixture.app(f'{index:07x}', index) for index in range(1, 8)]
        fixture.write('release/Vmux.app/Contents/MacOS/vmux_desktop', 'shipping app')
        fixture.write('release/Vmux Dev.app/Contents/MacOS/vmux_desktop', 'debug app')
        fixture.write('release/Vmux.dmg', 'shipping image')
        active = module.Processes([str(apps[1] / 'Contents/MacOS/vmux_desktop')], [])
        with patch.object(module.subprocess, 'run') as command:
            command.return_value.stdout = '0000001\n'
            fixture.storage.prune_apps(active, 'local')
        remaining = {path.name for path in apps if path.exists()}
        self.assertEqual(remaining, {'Vmux (0000001).app', 'Vmux (0000002).app', 'Vmux (0000006).app', 'Vmux (0000007).app'})
        self.assertEqual((fixture.target / 'release/Vmux.app/Contents/MacOS/vmux_desktop').read_text(), 'shipping app')
        self.assertEqual((fixture.target / 'release/Vmux.dmg').read_text(), 'shipping image')
        self.assertTrue((fixture.target / 'release/Vmux Dev.app').exists())

    def test_preflight_reclaims_idle_cache_before_capacity_check(self):
        fixture = self.fixture
        fixture.caches('debug')
        cached = fixture.target / 'debug/deps/object.o'
        with patch.object(fixture.storage, 'available', side_effect=lambda: (10 if cached.exists() else 60) * 1024**3):
            with patch.object(module.Processes, 'capture', return_value=fixture.idle):
                fixture.storage.preflight()
        self.assertFalse(cached.exists())

    def test_preflight_refuses_low_space_when_app_keeps_cache_busy(self):
        fixture = self.fixture
        fixture.caches('debug')
        active = module.Processes([str(fixture.target / 'debug/vmux_desktop')], [])
        with patch.object(fixture.storage, 'available', return_value=10 * 1024**3):
            with patch.object(module.Processes, 'capture', return_value=active):
                with self.assertRaisesRegex(RuntimeError, 'Insufficient build space'):
                    fixture.storage.preflight()
        self.assertTrue((fixture.target / 'debug/deps/object.o').exists())

    def test_success_prunes_cache_over_budget_and_keeps_release_output(self):
        fixture = self.fixture
        fixture.caches('release')
        fixture.write('release/Vmux.app/Contents/MacOS/vmux_desktop', 'shipping app')
        fixture.write('release/Vmux.dmg', 'shipping image')
        fixture.storage.policy = module.Policy(0, 0, 1, 3)
        with patch.object(module.Processes, 'capture', return_value=fixture.idle):
            fixture.storage.finish('release')
        self.assertFalse((fixture.target / 'release/deps/object.o').exists())
        self.assertEqual((fixture.target / 'release/Vmux.dmg').read_text(), 'shipping image')
        self.assertEqual((fixture.target / 'release/Vmux.app/Contents/MacOS/vmux_desktop').read_text(), 'shipping app')

    def test_workspace_and_symlink_target_are_refused(self):
        fixture = self.fixture
        with self.assertRaises(ValueError):
            module.Storage(fixture.root, fixture.root, fixture.policy)
        link = fixture.root / 'target'
        link.symlink_to(fixture.target, target_is_directory=True)
        with self.assertRaises(ValueError):
            module.Storage(fixture.root, link, fixture.policy)

    def test_process_inspection_failure_prevents_deletion(self):
        fixture = self.fixture
        fixture.caches('debug')
        with patch.object(fixture.storage, 'available', return_value=10 * 1024**3):
            with patch.object(module.Processes, 'capture', side_effect=RuntimeError('Cannot inspect')):
                with self.assertRaisesRegex(RuntimeError, 'Cannot inspect'):
                    fixture.storage.preflight()
        self.assertTrue((fixture.target / 'debug/deps/object.o').exists())

    def test_open_executable_is_protected_when_command_has_no_absolute_path(self):
        fixture = self.fixture
        executable = fixture.target / 'debug/Vmux Dev.app/Contents/MacOS/vmux_desktop'
        results = [
            SimpleNamespace(stdout='987654 vmux_desktop\n'),
            SimpleNamespace(returncode=0, stderr='', stdout=f'p987654\nn{executable}\n'),
        ]
        with patch.object(module.subprocess, 'run', side_effect=results):
            processes = module.Processes.capture()
        self.assertTrue(processes.uses(fixture.target / 'debug'))
        self.assertFalse(processes.uses(fixture.target / 'release'))

    def test_empty_open_path_snapshot_is_refused(self):
        results = [
            SimpleNamespace(stdout='987654 vmux_desktop\n'),
            SimpleNamespace(returncode=1, stderr='', stdout=''),
        ]
        with patch.object(module.subprocess, 'run', side_effect=results):
            with self.assertRaisesRegex(RuntimeError, 'Cannot inspect'):
                module.Processes.capture()

    def test_policy_rejects_nan_negative_and_inverted_thresholds(self):
        for env in [
            {'VMUX_BUILD_STORAGE_MIN_GIB': 'nan'},
            {'VMUX_BUILD_STORAGE_CACHE_GIB': '-1'},
            {'VMUX_BUILD_STORAGE_MIN_GIB': '60', 'VMUX_BUILD_STORAGE_WARN_GIB': '50'},
            {'VMUX_BUILD_STORAGE_LOCAL_KEEP': '0'},
        ]:
            with self.subTest(env=env), patch.dict(os.environ, env):
                with self.assertRaises(ValueError):
                    module.Policy.environment()


if __name__ == '__main__':
    unittest.main()
