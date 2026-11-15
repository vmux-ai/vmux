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
    const manifestPath = path.join(directory, 'manifest.json');
    if (fs.lstatSync(manifestPath).isSymbolicLink() || fs.statSync(manifestPath).size > 32768) throw new Error('Invalid CLI artifact manifest');
    const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
    const expected = this.identity(env, root);
    for (const [key, value] of Object.entries(expected)) {
      if (manifest[key] !== value) throw new Error(`CLI artifact ${key} does not match this job`);
    }
    const binary = path.join(directory, 'vmux');
    if (this.checksum(binary) !== manifest.sha256) throw new Error('CLI artifact checksum mismatch');
    const destination = path.join(root, '.build-tools/bin');
    for (const directory of [path.join(root, '.build-tools'), destination]) {
      if (fs.existsSync(directory) && fs.lstatSync(directory).isSymbolicLink()) throw new Error('CLI destination must not be a symlink');
    }
    fs.mkdirSync(destination, { recursive: true });
    const executable = path.join(destination, 'vmux');
    if (fs.lstatSync(executable, { throwIfNoEntry: false })?.isSymbolicLink()) throw new Error('CLI destination must not be a symlink');
    fs.copyFileSync(binary, executable);
    fs.chmodSync(executable, 0o755);
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
  else if (process.argv[2] === 'fast-rust') CliArtifact.configureFastRust();
  else throw new Error('Expected publish, install or fast-rust');
}
