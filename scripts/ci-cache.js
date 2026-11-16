const { execFileSync } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const Preflight = require('./ci-preflight.js');

class Sources {
  constructor(root, snapshot, target) {
    this.root = fs.realpathSync(root);
    this.snapshot = this.contained(snapshot);
    this.target = this.contained(target);
    this.validate(this.snapshot);
    this.validate(this.target, true);
  }

  validate(location, directory = false) {
    let current = this.root;
    for (const part of path.relative(this.root, location).split(path.sep)) {
      current = path.join(current, part);
      const stat = fs.lstatSync(current, { throwIfNoEntry: false });
      if (!stat) break;
      if (stat.isSymbolicLink()) throw new Error('Cache paths must not contain symbolic links');
      if (current !== location && !stat.isDirectory()) throw new Error('Cache parent must be a directory');
      if (current === location && directory && !stat.isDirectory()) throw new Error('Cache target must be a directory');
    }
  }

  contained(file) {
    const resolved = path.resolve(this.root, file);
    if (!resolved.startsWith(`${this.root}${path.sep}`)) throw new Error('Cache path escapes the checkout');
    return resolved;
  }

  git(...args) {
    return execFileSync('git', args, { cwd: this.root, env: Preflight.gitEnvironment(), encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  }

  files() {
    const names = this.git('ls-files', '-z');
    const files = new Map();
    for (const name of names.split('\0')) {
      if (!name) continue;
      const file = this.contained(name);
      if (!fs.existsSync(file)) continue;
      const stat = fs.lstatSync(file, { bigint: true });
      if (!stat.isFile()) continue;
      if (!fs.realpathSync(file).startsWith(`${this.root}${path.sep}`)) throw new Error('Source file escapes the checkout');
      files.set(name, { file, stat, hash: crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex') });
    }
    return files;
  }

  directories(files) {
    const directories = new Map();
    for (const [name, current] of files) {
      let directory = path.dirname(current.file);
      while (directory !== this.root) {
        const relative = path.relative(this.root, directory);
        if (!directories.has(relative)) directories.set(relative, { file: directory, hash: crypto.createHash('sha256') });
        directories.get(relative).hash.update(`${name}\0${current.hash}\0${current.stat.mode}\0`);
        directory = path.dirname(directory);
      }
    }
    for (const [name, directory] of directories) {
      let complete = true;
      for (const entry of fs.readdirSync(directory.file, { withFileTypes: true })) {
        const child = path.join(name, entry.name);
        if (entry.isFile() && files.has(child)) continue;
        if (entry.isDirectory() && directories.has(child)) continue;
        complete = false;
        break;
      }
      directory.stat = fs.statSync(directory.file, { bigint: true });
      directory.hash = complete ? directory.hash.digest('hex') : null;
    }
    return directories;
  }

  compiledAt() {
    let latest = 0n;
    const directories = [this.target];
    while (directories.length) {
      const directory = directories.pop();
      if (!fs.existsSync(directory)) continue;
      if (!fs.lstatSync(directory).isDirectory()) continue;
      for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
        const file = path.join(directory, entry.name);
        if (entry.isDirectory()) directories.push(file);
        if (entry.isFile() && entry.name === 'invoked.timestamp') {
          const time = fs.statSync(file, { bigint: true }).mtimeNs;
          if (time > latest) latest = time;
        }
      }
    }
    return latest;
  }

  invalidateLinks() {
    const names = this.git('ls-files', '-z').split('\0');
    let linked = false;
    for (const name of names) {
      if (!name) continue;
      const stat = fs.lstatSync(this.contained(name), { throwIfNoEntry: false });
      if (stat?.isSymbolicLink()) linked = true;
    }
    if (!linked) return;
    const directories = [this.target];
    while (directories.length) {
      const directory = directories.pop();
      if (!fs.existsSync(directory) || !fs.lstatSync(directory).isDirectory()) continue;
      for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
        if (!entry.isDirectory()) continue;
        const file = path.join(directory, entry.name);
        if (entry.name === '.fingerprint') fs.rmSync(file, { recursive: true });
        else directories.push(file);
      }
    }
  }

  read(file = this.snapshot) {
    file = this.contained(file);
    this.validate(file);
    if (!fs.existsSync(file)) return { files: {}, directories: {} };
    if (!fs.lstatSync(file).isFile()) throw new Error('Source snapshot must be a regular file');
    if (!fs.realpathSync(file).startsWith(`${this.root}${path.sep}`)) throw new Error('Source snapshot escapes the checkout');
    try {
      const record = JSON.parse(fs.readFileSync(file, 'utf8'));
      if (record.version !== 2 || !record.files || Array.isArray(record.files) || typeof record.files !== 'object') return { files: {}, directories: {} };
      if (!record.directories || Array.isArray(record.directories) || typeof record.directories !== 'object') record.directories = {};
      return record;
    } catch {
      return { files: {}, directories: {} };
    }
  }

  restore() {
    this.invalidateLinks();
    const saved = this.read();
    const compiledAt = this.compiledAt() + 1000000000n;
    const now = BigInt(Date.now()) * 1000000n;
    const changedAt = now > compiledAt ? now : compiledAt;
    let restored = 0;
    let changed = 0;
    const files = this.files();
    for (const [name, current] of files) {
      const previous = Object.hasOwn(saved.files, name) ? saved.files[name] : undefined;
      let time = changedAt;
      if (previous?.hash === current.hash && previous.mode === Number(current.stat.mode) && typeof previous.mtime === 'string' && /^\d+$/.test(previous.mtime)) {
        time = BigInt(previous.mtime);
        restored++;
      } else {
        changed++;
      }
      fs.utimesSync(current.file, Number(current.stat.atimeNs) / 1e9, Number(time) / 1e9);
    }
    for (const [name, current] of this.directories(files)) {
      const previous = Object.hasOwn(saved.directories, name) ? saved.directories[name] : undefined;
      let time = changedAt;
      if (current.hash && previous?.hash === current.hash && typeof previous.mtime === 'string' && /^\d+$/.test(previous.mtime)) time = BigInt(previous.mtime);
      fs.utimesSync(current.file, Number(current.stat.atimeNs) / 1e9, Number(time) / 1e9);
    }
    const refs = ['HEAD', 'logs/HEAD'];
    try {
      refs.push(this.git('symbolic-ref', '--quiet', 'HEAD').trim());
    } catch (error) {
      if (error.status !== 1) throw error;
    }
    for (const ref of refs) {
      const file = this.contained(this.git('rev-parse', '--git-path', ref).trim());
      this.validate(file);
      if (!fs.existsSync(file)) continue;
      const stat = fs.lstatSync(file, { bigint: true });
      if (!stat.isFile()) continue;
      fs.utimesSync(file, Number(stat.atimeNs) / 1e9, Number(changedAt) / 1e9);
    }
    this.restored = this.capture();
    return { restored, changed };
  }

  capture() {
    const current = this.files();
    const files = Object.create(null);
    for (const [name, file] of current) {
      files[name] = { hash: file.hash, mode: Number(file.stat.mode), mtime: String(file.stat.mtimeNs) };
    }
    const directories = Object.create(null);
    for (const [name, directory] of this.directories(current)) directories[name] = { hash: directory.hash, mtime: String(directory.stat.mtimeNs) };
    return { version: 2, files, directories };
  }

  save(restored = this.restored) {
    const record = this.capture();
    if (restored) {
      const compiledAt = this.compiledAt() + 1000000000n;
      const now = BigInt(Date.now()) * 1000000n;
      const changedAt = String(now > compiledAt ? now : compiledAt);
      for (const [name, file] of Object.entries(record.files)) {
        const previous = Object.hasOwn(restored.files, name) ? restored.files[name] : undefined;
        file.mtime = previous?.hash === file.hash && previous.mode === file.mode ? previous.mtime : changedAt;
      }
      for (const [name, directory] of Object.entries(record.directories)) {
        const previous = Object.hasOwn(restored.directories, name) ? restored.directories[name] : undefined;
        directory.mtime = directory.hash && previous?.hash === directory.hash ? previous.mtime : changedAt;
      }
    }
    this.write(this.snapshot, record);
  }

  write(file, record) {
    file = this.contained(file);
    this.validate(file);
    let parent = path.dirname(file);
    while (!fs.existsSync(parent)) parent = path.dirname(parent);
    const existingParent = fs.realpathSync(parent);
    if (existingParent !== this.root && !existingParent.startsWith(`${this.root}${path.sep}`)) throw new Error('Snapshot directory escapes the checkout');
    fs.mkdirSync(path.dirname(file), { recursive: true });
    const directory = fs.realpathSync(path.dirname(file));
    if (directory !== this.root && !directory.startsWith(`${this.root}${path.sep}`)) throw new Error('Snapshot directory escapes the checkout');
    const temporary = `${file}.${crypto.randomUUID()}`;
    let owned = false;
    try {
      fs.writeFileSync(temporary, `${JSON.stringify(record)}\n`, { flag: 'wx' });
      owned = true;
      fs.renameSync(temporary, file);
    } finally {
      if (owned) fs.rmSync(temporary, { force: true });
    }
  }

  inputs(env) {
    const partition = env.VMUX_CACHE_PARTITION;
    if (!/^[a-zA-Z0-9_-]+$/.test(partition)) throw new Error('Invalid Cargo cache partition');
    if (!/^[a-f0-9]{40}$/.test(env.GITHUB_SHA)) throw new Error('Invalid Cargo cache commit');
    const manifest = this.contained(path.join(env.VMUX_CACHE_WORKSPACE || '.', 'Cargo.toml'));
    const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--no-deps', '--format-version=1', '--manifest-path', manifest], { cwd: this.root, env: Preflight.gitEnvironment(), encoding: 'utf8' }));
    const members = new Set(metadata.workspace_members);
    const packages = metadata.packages.filter((item) => members.has(item.id));
    if (!packages.length) throw new Error('No workspace packages to cache');
    const target = path.relative(this.root, this.target);
    const paths = new Set([path.relative(this.root, this.snapshot)]);
    const profiles = new Set(['debug', 'release']);
    for (const match of fs.readFileSync(manifest, 'utf8').matchAll(/^\[profile\.([a-zA-Z0-9_-]+)\]\s*$/gm)) {
      const name = match[1];
      profiles.add(name === 'dev' || name === 'test' ? 'debug' : name === 'bench' ? 'release' : name);
    }
    const outputs = [];
    for (const profile of profiles) outputs.push(`${target}/${profile}`, `${target}/*/${profile}`);
    const hash = crypto.createHash('sha256');
    hash.update(execFileSync('rustc', ['-vV'], { encoding: 'utf8' }));
    for (const key of Object.keys(env).sort()) {
      if (/^(RUSTFLAGS$|RUSTC$|RUSTC_WRAPPER$|CARGO_ENCODED_RUSTFLAGS$|CARGO_BUILD_|CARGO_TARGET_|CARGO_PROFILE_|VMUX_CI_VALIDATION$|VMUX_BUILD_PROFILE$|VMUX_HOST_NAME$|CC$|CXX$|CFLAGS$|CXXFLAGS$|AR$)/.test(key)) {
        hash.update(`${key}=${env[key]}\0`);
      }
    }
    for (const pkg of packages) {
      if (!/^[a-zA-Z0-9_-]+$/.test(pkg.name)) throw new Error('Invalid workspace package name');
      const manifest = this.contained(pkg.manifest_path);
      hash.update(`${path.relative(this.root, manifest)}\0`);
      hash.update(fs.readFileSync(manifest));
      for (const output of outputs) {
        paths.add(`${output}/.fingerprint/${pkg.name}-*`);
        paths.add(`${output}/build/${pkg.name}-*`);
      }
      for (const item of pkg.targets) {
        if (!/^[a-zA-Z0-9_-]+$/.test(item.name)) throw new Error('Invalid workspace target name');
        const name = item.name.replaceAll('-', '_');
        for (const output of outputs) {
          paths.add(`${output}/deps/${name}-*`);
          paths.add(`${output}/deps/lib${name}-*`);
          if (item.kind.includes('example')) {
            paths.add(`${output}/examples/${name}*`);
            paths.add(`${output}/examples/lib${name}*`);
          }
          paths.add(`${output}/${item.name}*`);
          paths.add(`${output}/lib${name}*`);
        }
      }
    }
    for (const extension of ['app', 'dmg', 'zip', 'tar', 'tar.*']) {
      paths.add(`!${target}/**/*.${extension}`);
      paths.add(`!${target}/**/*.${extension}/**`);
    }
    for (const file of new Set(['Cargo.lock', path.join(env.VMUX_CACHE_WORKSPACE || '.', 'Cargo.lock'), 'rust-toolchain.toml', 'tool-versions.env', '.cargo/config.toml'])) {
      const location = this.contained(file);
      if (fs.existsSync(location)) hash.update(fs.readFileSync(location));
    }
    const prefix = `vmux-source-v2-${env.RUNNER_OS}-${env.RUNNER_ARCH}-${partition}-${hash.digest('hex')}`;
    return { prefix, key: `${prefix}-${env.GITHUB_SHA}`, paths: [...paths].join('\n'), snapshot: path.relative(this.root, this.snapshot), target };
  }

  static run(env = process.env) {
    const post = env.STATE_cache_sources_post === 'true';
    const inputs = post ? JSON.parse(env.STATE_cache_sources_inputs) : {
      workspace: env.INPUT_WORKSPACE || '.',
      snapshot: env.INPUT_SNAPSHOT,
      target: env.INPUT_TARGET,
      save: env.INPUT_SAVE === 'true',
    };
    const root = this.workspace(env.GITHUB_WORKSPACE || process.cwd(), inputs.workspace || '.');
    const sources = new Sources(root, inputs.snapshot, inputs.target);
    if (post) {
      if (inputs.save) {
        const receipt = sources.contained(inputs.receipt);
        sources.validate(receipt);
        if (!fs.existsSync(receipt)) return;
        try {
          sources.save(sources.read(receipt));
        } finally {
          sources.validate(receipt);
          fs.rmSync(receipt, { force: true });
        }
      }
      return;
    }
    const result = sources.restore();
    if (inputs.save) {
      inputs.receipt = env.INPUT_RECEIPT || `${inputs.snapshot}.${crypto.randomUUID()}.restore`;
      sources.write(inputs.receipt, sources.restored);
    }
    fs.appendFileSync(env.GITHUB_STATE, `cache_sources_post=true\ncache_sources_inputs=${JSON.stringify(inputs)}\n`);
    process.stdout.write(`Cargo source cache: ${result.restored} unchanged, ${result.changed} invalidated\n`);
  }

  static workspace(root, directory) {
    root = fs.realpathSync(root);
    const resolved = path.resolve(root, directory);
    if (resolved === root) return root;
    if (!resolved.startsWith(`${root}${path.sep}`)) throw new Error('Source workspace escapes the checkout');
    let current = root;
    for (const part of path.relative(root, resolved).split(path.sep)) {
      current = path.join(current, part);
      const stat = fs.lstatSync(current);
      if (stat.isSymbolicLink() || !stat.isDirectory()) throw new Error('Source workspace must contain only real directories');
    }
    return resolved;
  }

  static configure(env = process.env) {
    const sources = new Sources(env.GITHUB_WORKSPACE || process.cwd(), `.build-tools/source-cache-${env.VMUX_CACHE_PARTITION}.json`, env.VMUX_CACHE_TARGET);
    const inputs = { ...sources.inputs(env), receipt: `${path.relative(sources.root, sources.snapshot)}.${crypto.randomUUID()}.restore`, save: env.VMUX_CACHE_SAVE === 'true' };
    sources.write(`${sources.snapshot}.inputs`, inputs);
    this.output(inputs, env);
  }

  static checkpoint(env = process.env) {
    const partition = env.VMUX_CACHE_PARTITION;
    if (!/^[a-zA-Z0-9_-]+$/.test(partition)) throw new Error('Invalid Cargo cache partition');
    const root = env.GITHUB_WORKSPACE || process.cwd();
    const record = `.build-tools/source-cache-${partition}.json.inputs`;
    const guard = new Sources(root, record, 'target');
    guard.validate(guard.snapshot);
    const inputs = JSON.parse(fs.readFileSync(guard.snapshot, 'utf8'));
    const sources = new Sources(root, inputs.snapshot, inputs.target);
    if (inputs.save) {
      const receipt = sources.contained(inputs.receipt);
      sources.validate(receipt);
      if (!fs.existsSync(receipt)) throw new Error('Missing source-cache restore receipt');
      sources.save(sources.read(receipt));
      fs.rmSync(receipt);
    }
    this.output(inputs, env);
  }

  static output(inputs, env) {
    for (const [name, value] of Object.entries(inputs)) {
      const delimiter = crypto.randomUUID();
      fs.appendFileSync(env.GITHUB_OUTPUT, `${name}<<${delimiter}\n${value}\n${delimiter}\n`);
    }
  }

  static verify(env = process.env) {
    const root = env.GITHUB_WORKSPACE || process.cwd();
    for (const target of new Set(['target', '.build-tools/cargo', env.VMUX_CACHE_TARGET].filter(Boolean))) {
      new Sources(root, '.build-tools/source-cache-validation.json', target);
    }
  }
}

module.exports = Sources;

if (require.main === module) {
  if (process.argv[2] === 'inputs') Sources.configure();
  else if (process.argv[2] === 'validate') Sources.verify();
  else if (process.argv[2] === 'checkpoint') Sources.checkpoint();
  else Sources.run();
}
