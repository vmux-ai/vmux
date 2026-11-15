const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const Preflight = require('./ci-preflight.js');

class CliArtifact {
  static identity(env = process.env, root = process.cwd()) {
    const toolchain = fs.readFileSync(path.join(root, 'rust-toolchain.toml'), 'utf8').match(/^channel\s*=\s*"([^"]+)"/m)?.[1];
    const tree = execFileSync('git', ['rev-parse', 'HEAD^{tree}'], { cwd: root, env: Preflight.gitEnvironment(), encoding: 'utf8' }).trim();
    Preflight.sha(tree);
    if (!toolchain || !/^\d+$/.test(env.GITHUB_RUN_ID || '') || !/^[^/]+\/[^/]+$/.test(env.GITHUB_REPOSITORY || '')) throw new Error('Missing CLI artifact provenance');
    if (!['Linux', 'macOS'].includes(env.RUNNER_OS) || !['X64', 'ARM64'].includes(env.RUNNER_ARCH)) throw new Error('Unsupported CLI artifact platform');
    return { version: 1, repository: env.GITHUB_REPOSITORY, run: env.GITHUB_RUN_ID, tree, toolchain, os: env.RUNNER_OS, arch: env.RUNNER_ARCH, features: 'dx,ci-syntax', compile: env.VMUX_CI_FAST_RUST === '1' ? 'opt1-cgu16' : 'default' };
  }

  static checksum(file) {
    const metadata = fs.lstatSync(file);
    if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size === 0 || metadata.size > 256 * 1024 * 1024) throw new Error('Invalid development CLI binary');
    return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  }

  static publish(binary, directory, env = process.env, root = process.cwd()) {
    const identity = this.identity(env, root);
    const sha256 = this.checksum(binary);
    fs.mkdirSync(directory, { recursive: true });
    fs.copyFileSync(binary, path.join(directory, 'vmux'));
    fs.writeFileSync(path.join(directory, 'manifest.json'), `${JSON.stringify({ ...identity, sha256 })}\n`);
  }

  static install(directory, env = process.env, root = process.cwd()) {
    const manifest = this.readManifest(directory);
    const expected = this.identity(env, root);
    for (const [key, value] of Object.entries(expected)) {
      if (manifest[key] !== value) throw new Error(`CLI artifact ${key} does not match this job`);
    }
    const binary = path.join(directory, 'vmux');
    if (this.checksum(binary) !== manifest.sha256) throw new Error('CLI artifact checksum mismatch');
    this.installBinary(binary, root);
  }

  static readManifest(directory) {
    const manifestPath = path.join(directory, 'manifest.json');
    const stat = fs.lstatSync(manifestPath);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 32768) throw new Error('Invalid CLI artifact manifest');
    return JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  }

  static installBinary(binary, root) {
    const destination = path.join(root, '.build-tools/bin');
    for (const directory of [path.join(root, '.build-tools'), destination]) {
      if (fs.lstatSync(directory, { throwIfNoEntry: false })?.isSymbolicLink()) throw new Error('CLI destination must not be a symlink');
    }
    fs.mkdirSync(destination, { recursive: true });
    const executable = path.join(destination, 'vmux');
    if (fs.lstatSync(executable, { throwIfNoEntry: false })?.isSymbolicLink()) throw new Error('CLI destination must not be a symlink');
    fs.copyFileSync(binary, executable);
    fs.chmodSync(executable, 0o755);
  }

  static cacheIdentity(env = process.env, root = process.cwd()) {
    root = fs.realpathSync(root);
    const { run, tree, ...identity } = this.identity(env, root);
    const compiler = execFileSync('rustc', ['-vV'], { encoding: 'utf8' });
    const host = compiler.match(/^host: (.+)$/m)?.[1];
    if (!host) throw new Error('Missing Rust compiler host');
    const graph = execFileSync('cargo', ['tree', '--locked', '-p', 'vmux_cli', '--no-default-features', '--features', identity.features, '--target', host, '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}'], { cwd: root, env: Preflight.gitEnvironment(), encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
    const directories = new Set();
    for (const line of graph.split('\n')) {
      const location = line.replace(/ \(\*\)$/, '').match(/ \((\/.*)\)$/)?.[1];
      if (!location) continue;
      const directory = fs.realpathSync(location);
      if (!directory.startsWith(`${root}${path.sep}`)) throw new Error('CLI dependency escapes the checkout');
      directories.add(`${path.relative(root, directory)}/`);
    }
    if (!directories.has('crates/app/vmux_cli/')) throw new Error('CLI dependency graph lacks its root');
    const hash = crypto.createHash('sha256');
    hash.update(JSON.stringify(identity));
    hash.update(compiler);
    hash.update(execFileSync('cargo', ['-V'], { encoding: 'utf8' }));
    hash.update(graph.replaceAll(root, '<checkout>'));
    for (const key of Object.keys(env).sort()) {
      if (/^(CARGO_|RUSTC|CC$|CXX$|CFLAGS$|CXXFLAGS$|AR$|VMUX_BUILD_PROFILE$|VMUX_HOST_NAME$|VMUX_TARGET_DESTINATION$|VMUX_UPDATE_PUBLIC_KEY$|ImageOS$|ImageVersion$)/.test(key)) hash.update(`${key}=${env[key]}\0`);
    }
    const names = execFileSync('git', ['ls-files', '-z'], { cwd: root, env: Preflight.gitEnvironment(), encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 }).split('\0').filter(Boolean).sort();
    const required = new Set(['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'tool-versions.env']);
    for (const name of names) {
      const shared = required.has(name) || name.startsWith('.cargo/') || /^crates\/build[^/]*\.rs$/.test(name) || name === 'scripts/ci-cli.js' || name === '.github/actions/setup-rust/action.yml';
      if (!shared && ![...directories].some((directory) => name.startsWith(directory))) continue;
      const file = path.join(root, name);
      const stat = fs.lstatSync(file);
      if (!stat.isFile() || !fs.realpathSync(file).startsWith(`${root}${path.sep}`)) throw new Error('Invalid CLI compile input');
      hash.update(`${name}\0${stat.mode}\0`);
      hash.update(crypto.createHash('sha256').update(fs.readFileSync(file)).digest());
      required.delete(name);
    }
    if (required.size) throw new Error('Missing tracked CLI compile inputs');
    return { ...identity, input: hash.digest('hex') };
  }

  static prepareCache(env = process.env, root = process.cwd()) {
    const identity = this.cacheIdentity(env, root);
    const directory = path.join(root, '.build-tools');
    if (fs.lstatSync(directory, { throwIfNoEntry: false })?.isSymbolicLink()) throw new Error('CLI destination must not be a symlink');
    fs.mkdirSync(directory, { recursive: true });
    const record = path.join(directory, 'cli-inputs.json');
    if (fs.lstatSync(record, { throwIfNoEntry: false })?.isSymbolicLink()) throw new Error('CLI input record must not be a symlink');
    fs.writeFileSync(record, JSON.stringify(identity));
    fs.appendFileSync(env.GITHUB_OUTPUT, `key=vmux-cli-v1-${identity.os}-${identity.arch}-${identity.input}\n`);
  }

  static cachedIdentity(root) {
    return JSON.parse(fs.readFileSync(path.join(root, '.build-tools/cli-inputs.json'), 'utf8'));
  }

  static publishCache(directory, env = process.env, root = process.cwd()) {
    const identity = this.cachedIdentity(root);
    const binary = path.join(root, '.build-tools/bin/vmux');
    const sha256 = this.checksum(binary);
    if (path.resolve(directory) !== path.resolve(root, '.build-tools/cli-cache')) throw new Error('CLI cache path must be runner-owned');
    if (fs.lstatSync(path.join(root, '.build-tools'))?.isSymbolicLink()) throw new Error('CLI cache parent must not be a symlink');
    if (fs.lstatSync(directory, { throwIfNoEntry: false })?.isSymbolicLink()) fs.unlinkSync(directory);
    fs.mkdirSync(directory, { recursive: true });
    for (const name of ['vmux', 'manifest.json']) {
      if (fs.lstatSync(path.join(directory, name), { throwIfNoEntry: false })?.isSymbolicLink()) fs.unlinkSync(path.join(directory, name));
    }
    fs.copyFileSync(binary, path.join(directory, 'vmux'));
    fs.writeFileSync(path.join(directory, 'manifest.json'), `${JSON.stringify({ ...identity, sha256 })}\n`);
  }

  static reuseCache(directory, env = process.env, root = process.cwd()) {
    let reusable = false;
    try {
      if (fs.lstatSync(directory).isSymbolicLink()) throw new Error('CLI cache must not be a symlink');
      const manifest = this.readManifest(directory);
      for (const [key, value] of Object.entries(this.cachedIdentity(root))) {
        if (manifest[key] !== value) throw new Error(`Cached CLI ${key} does not match compile inputs`);
      }
      const binary = path.join(directory, 'vmux');
      if (this.checksum(binary) !== manifest.sha256) throw new Error('Cached CLI checksum mismatch');
      this.installBinary(binary, root);
      reusable = true;
    } catch (error) {
      process.stdout.write(`Development CLI cache rejected or absent: ${error.message}\n`);
    }
    fs.appendFileSync(env.GITHUB_OUTPUT, `reused=${reusable}\n`);
    if (reusable) process.stdout.write('Reused verified development CLI; Cargo build skipped\n');
    return reusable;
  }

  static configureFastRust(env = process.env) {
    const result = { CARGO_PROFILE_DEV_LTO: 'off' };
    if (env.CARGO_ENCODED_RUSTFLAGS !== undefined) result.CARGO_ENCODED_RUSTFLAGS = `${env.CARGO_ENCODED_RUSTFLAGS ? `${env.CARGO_ENCODED_RUSTFLAGS}\x1f` : ''}-C\x1fopt-level=1\x1f-C\x1fcodegen-units=16`;
    else result.RUSTFLAGS = `${env.RUSTFLAGS ? `${env.RUSTFLAGS} ` : ''}-C opt-level=1 -C codegen-units=16`;
    if (!env.GITHUB_ENV) throw new Error('Missing GitHub environment output');
    for (const value of Object.values(result)) {
      if (/[\r\n]/.test(value)) throw new Error('Invalid multiline Rust compile flags');
    }
    for (const [key, value] of Object.entries(result)) fs.appendFileSync(env.GITHUB_ENV, `${key}=${value}\n`);
  }
}

module.exports = CliArtifact;

if (require.main === module) {
  if (process.argv[2] === 'publish') CliArtifact.publish(process.argv[3], process.argv[4]);
  else if (process.argv[2] === 'install') CliArtifact.install(process.argv[3]);
  else if (process.argv[2] === 'cache-inputs') CliArtifact.prepareCache();
  else if (process.argv[2] === 'cache-publish') CliArtifact.publishCache(process.argv[3]);
  else if (process.argv[2] === 'cache-reuse') CliArtifact.reuseCache(process.argv[3]);
  else if (process.argv[2] === 'fast-rust') CliArtifact.configureFastRust();
  else throw new Error('Expected publish, install, cache-inputs, cache-publish, cache-reuse or fast-rust');
}
