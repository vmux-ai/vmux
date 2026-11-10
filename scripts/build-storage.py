#!/usr/bin/env python3
import argparse
from dataclasses import dataclass
import fcntl
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys


@dataclass(frozen=True)
class Policy:
    minimum: int
    warning: int
    cache_limit: int
    local_keep: int

    @classmethod
    def environment(cls):
        values = []
        for name, default in [
            ('VMUX_BUILD_STORAGE_MIN_GIB', '20'),
            ('VMUX_BUILD_STORAGE_WARN_GIB', '50'),
            ('VMUX_BUILD_STORAGE_CACHE_GIB', '50'),
        ]:
            value = float(os.environ.get(name, default))
            if not math.isfinite(value) or value < 0:
                raise ValueError(f'{name} must be a finite nonnegative number')
            values.append(int(value * 1024**3))
        keep = int(os.environ.get('VMUX_BUILD_STORAGE_LOCAL_KEEP', '3'))
        if keep < 1 or values[1] < values[0]:
            raise ValueError('Keep at least one local app and set warning >= minimum')
        return cls(*values, keep)


class Processes:
    def __init__(self, commands, paths):
        self.commands = commands
        self.paths = paths

    @classmethod
    def capture(cls):
        result = subprocess.run(['ps', '-axo', 'pid=,args='], check=True, capture_output=True, text=True)
        commands = []
        for line in result.stdout.splitlines():
            parts = line.strip().split(None, 1)
            if len(parts) == 2 and int(parts[0]) != os.getpid():
                commands.append(parts[1])
        result = subprocess.run(
            ['lsof', '-n', '-P', '-a', '-u', str(os.getuid()), '-d', 'txt,cwd', '-Fpn'],
            capture_output=True, text=True,
        )
        if result.returncode not in (0, 1) or result.stderr.strip():
            raise RuntimeError(f'Cannot inspect running artifacts: {result.stderr.strip()}')
        if not result.stdout.strip():
            raise RuntimeError('Cannot inspect running artifacts: lsof returned no processes')
        paths = []
        pid = None
        for line in result.stdout.splitlines():
            if line.startswith('p'):
                pid = int(line[1:])
            elif line.startswith('n') and pid != os.getpid():
                paths.append(line[1:])
        if not paths:
            raise RuntimeError('Cannot inspect running artifacts: lsof returned no open paths')
        return cls(commands, paths)

    def uses(self, path):
        value = str(path)
        for command in self.commands:
            if value in command:
                return True
        for opened in self.paths:
            if opened == value or opened.startswith(value + os.sep):
                return True
        return False


class Storage:
    cache_names = ('.fingerprint', 'deps', 'build', 'incremental', 'examples')
    profile_names = ('debug', 'release', 'cef-helper')
    local_name = re.compile(r'Vmux \([0-9a-f]{7,40}\)\.app')

    def __init__(self, root, target, policy):
        self.root = Path(root).resolve()
        self.target = Path(target).absolute()
        self.policy = policy
        if self.target.resolve() != self.target or self.target == self.root or self.root.is_relative_to(self.target):
            raise ValueError('Build storage requires a dedicated, nonsymlink Cargo target directory')

    def available(self):
        path = self.target
        while not path.exists():
            path = path.parent
        return shutil.disk_usage(path).free

    def profiles(self):
        roots = [self.target]
        if self.target.is_dir():
            for entry in self.target.iterdir():
                if entry.is_dir() and not entry.is_symlink():
                    roots.append(entry)
        profiles = []
        for base in roots:
            for name in self.profile_names:
                path = base / name
                if path.is_dir() and not path.is_symlink():
                    profiles.append(path)
        return profiles

    def cache_paths(self, profile):
        paths = []
        for name in self.cache_names:
            path = profile / name
            if path.is_dir() and not path.is_symlink():
                paths.append(path)
        return paths

    def cache_bytes(self):
        paths = []
        for profile in self.profiles():
            paths.extend(self.cache_paths(profile))
        if not paths:
            return 0
        result = subprocess.run(['du', '-sk', *map(str, paths)], check=True, capture_output=True, text=True)
        total = 0
        for line in result.stdout.splitlines():
            total += int(line.split()[0]) * 1024
        return total

    def cargo_target(self):
        tag = self.target / 'CACHEDIR.TAG'
        return tag.is_file() and 'Signature: 8a477f597d28d172789f06886806bc55' in tag.read_text()

    def prune_caches(self, processes, apply=True):
        if not self.cargo_target():
            print('Build storage: no Cargo cache tag; cache deletion skipped')
            return
        removed = False
        for profile in self.profiles():
            if processes.uses(profile):
                print(f'Build storage: keeping active profile {profile}')
                continue
            with open(profile / '.cargo-lock', 'a+') as lock:
                try:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError:
                    print(f'Build storage: keeping locked profile {profile}')
                    continue
                for path in self.cache_paths(profile):
                    print(f'Build storage: {"removing" if apply else "would remove"} {path}' +
                          (' (keeping dylibs)' if path.name == 'deps' else ''))
                    if not apply:
                        continue
                    if path.name == 'deps':
                        for entry in path.iterdir():
                            if entry.name.endswith('.dylib') or entry.name.endswith('.so'):
                                continue
                            self.remove(entry)
                    else:
                        self.remove(path)
                    removed = True
        marker = self.target / '.vmux-seed'
        if removed and marker.is_dir() and not marker.is_symlink():
            self.remove(marker)

    def remove(self, path):
        if not path.is_relative_to(self.target):
            raise ValueError(f'Refusing to delete outside Cargo target: {path}')
        if path.is_symlink() or not path.is_dir():
            path.unlink()
        else:
            shutil.rmtree(path)

    def prune_apps(self, processes, profile):
        if not self.cargo_target():
            print('Build storage: no Cargo cache tag; app deletion skipped')
            return
        current = None
        if profile == 'local':
            revision = subprocess.run(
                ['git', '-C', str(self.root), 'rev-parse', '--short=7', 'HEAD'],
                check=True, capture_output=True, text=True,
            ).stdout.strip()
            current = f'Vmux ({revision}).app'
        for directory in self.profiles():
            if directory.name != 'release':
                continue
            apps = []
            for path in directory.iterdir():
                if self.local_name.fullmatch(path.name) and path.is_dir() and not path.is_symlink():
                    apps.append(path)
            apps.sort(key=lambda path: path.stat().st_mtime_ns, reverse=True)
            kept = set()
            for path in apps:
                if path.name == current:
                    kept.add(path)
            for path in apps:
                if len(kept) >= self.policy.local_keep:
                    break
                kept.add(path)
            for path in apps:
                if path in kept or processes.uses(path):
                    continue
                print(f'Build storage: removing old local app {path}')
                self.remove(path)

    def check(self):
        free = self.available()
        print(f'Build storage: {free / 1024**3:.1f} GiB free')
        if free < self.policy.minimum:
            raise RuntimeError(
                f'Insufficient build space: need {self.policy.minimum / 1024**3:g} GiB free; '
                'stop unused apps/builds and run make clean-build-cache with VMUX_BUILD_STORAGE_APPLY=1'
            )
        if free < self.policy.warning:
            print(f'Build storage: warning, less than {self.policy.warning / 1024**3:g} GiB free', file=sys.stderr)

    def preflight(self):
        if self.available() < self.policy.minimum:
            self.prune_caches(Processes.capture())
        self.check()

    def finish(self, profile):
        processes = Processes.capture()
        if profile == 'local':
            self.prune_apps(processes, profile)
        size = self.cache_bytes()
        if size > self.policy.cache_limit or self.available() < self.policy.minimum:
            print(f'Build storage: cache uses {size / 1024**3:.1f} GiB; pruning unused profiles')
            self.prune_caches(processes)
        free = self.available()
        print(f'Build storage: {free / 1024**3:.1f} GiB free after {profile} build')
        if free < self.policy.warning:
            print('Build storage: free space remains low; active profiles and distribution artifacts were kept',
                  file=sys.stderr)

    def report(self):
        print(f'Build storage: {self.available() / 1024**3:.1f} GiB free, '
              f'{self.cache_bytes() / 1024**3:.1f} GiB of compile caches')

    @classmethod
    def main(cls):
        parser = argparse.ArgumentParser()
        parser.add_argument('--root', required=True)
        parser.add_argument('--target', required=True)
        parser.add_argument('phase', choices=['check', 'preflight', 'finish', 'status', 'clean'])
        parser.add_argument('--profile', choices=['dev', 'local', 'release'], default='dev')
        args = parser.parse_args()
        storage = cls(args.root, args.target, Policy.environment())
        if args.phase == 'check':
            storage.check()
        elif args.phase == 'preflight':
            storage.preflight()
        elif args.phase == 'finish':
            storage.finish(args.profile)
        elif args.phase == 'status':
            storage.report()
        else:
            storage.prune_caches(Processes.capture(), os.environ.get('VMUX_BUILD_STORAGE_APPLY') == '1')
            storage.report()


if __name__ == '__main__':
    try:
        Storage.main()
    except (ValueError, RuntimeError, OSError, subprocess.SubprocessError) as error:
        print(f'Build storage: {error}', file=sys.stderr)
        sys.exit(1)
