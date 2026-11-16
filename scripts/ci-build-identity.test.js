const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

class BuildFixture {
  constructor() {
    this.root = fs.mkdtempSync(path.join(os.tmpdir(), 'vmux-ci-identity-'));
    fs.mkdirSync(path.join(this.root, 'src'));
    fs.copyFileSync(path.join(__dirname, '../crates/build_git_env.rs'), path.join(this.root, 'build_git_env.rs'));
    fs.writeFileSync(path.join(this.root, 'Cargo.toml'), '[package]\nname = "identity-fixture"\nversion = "0.1.0"\nedition = "2024"\n\n[workspace]\n');
    fs.writeFileSync(path.join(this.root, 'build.rs'), 'mod build_git_env;\nfn main() { build_git_env::emit(); }\n');
    this.source('initial');
    this.git('init', '-q', '-b', 'main');
    this.git('config', 'user.name', 'Identity Fixture');
    this.git('config', 'user.email', 'fixture@example.com');
    this.git('config', 'commit.gpgsign', 'false');
    this.git('add', '.');
    this.git('commit', '-qm', 'initial');
  }

  git(...args) {
    const env = { ...process.env, GIT_CONFIG_GLOBAL: '/dev/null', GIT_CONFIG_SYSTEM: '/dev/null' };
    for (const key of ['GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE']) delete env[key];
    return execFileSync('git', args, { cwd: this.root, env, encoding: 'utf8' }).trim();
  }

  source(marker) {
    fs.writeFileSync(path.join(this.root, 'src/main.rs'), `fn main() { println!("{}|{}|{}|${marker}", env!("VMUX_GIT_HASH"), env!("VMUX_GIT_BRANCH"), env!("VMUX_WORKTREE_ID")); }\n`);
  }

  build(validation, release = false, productProfile = '') {
    const env = { ...process.env, CARGO_TARGET_DIR: path.join(this.root, 'target'), CARGO_INCREMENTAL: '0', VMUX_CI_VALIDATION: validation ? '1' : '' };
    for (const key of ['CARGO_BUILD_TARGET', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'VMUX_BUILD_PROFILE', 'VMUX_HOST_NAME']) delete env[key];
    if (productProfile) env.VMUX_BUILD_PROFILE = productProfile;
    const args = ['build', '--offline', '--quiet'];
    if (release) args.push('--release');
    execFileSync('cargo', args, { cwd: this.root, env, stdio: ['ignore', 'pipe', 'pipe'] });
    const binary = path.join(this.root, `target/${release ? 'release' : 'debug'}/identity-fixture`);
    return { output: execFileSync(binary, { encoding: 'utf8' }).trim(), modified: fs.statSync(binary, { bigint: true }).mtimeNs };
  }

  close() {
    fs.rmSync(this.root, { recursive: true, force: true });
  }
}

test('validation builds reuse compiled output after Git-only changes and rebuild changed source', () => {
  const fixture = new BuildFixture();
  try {
    const first = fixture.build(true);
    fixture.git('commit', '--allow-empty', '-qm', 'new Git identity');
    const second = fixture.build(true);
    assert.equal(second.output, first.output);
    assert.equal(second.modified, first.modified);
    fixture.source('changed');
    const changed = fixture.build(true);
    assert.equal(changed.output.split('|').at(-1), 'changed');
    assert.notEqual(changed.modified, second.modified);
  } finally {
    fixture.close();
  }
});

test('product builds retain current Git identity and cannot reuse validation identity', () => {
  const fixture = new BuildFixture();
  try {
    const validation = fixture.build(true);
    const product = fixture.build(false);
    assert.equal(product.output.split('|')[0], fixture.git('rev-parse', '--short=7', 'HEAD'));
    assert.notEqual(product.output, validation.output);
    fixture.git('commit', '--allow-empty', '-qm', 'next product identity');
    const next = fixture.build(false);
    assert.equal(next.output.split('|')[0], fixture.git('rev-parse', '--short=7', 'HEAD'));
    assert.notEqual(next.output, product.output);
  } finally {
    fixture.close();
  }
});

test('release and local products preserve real Git identity even when CI validation is enabled', () => {
  for (const [release, profile] of [[true, 'release'], [false, 'local']]) {
    const fixture = new BuildFixture();
    try {
      const validation = fixture.build(true);
      const product = fixture.build(true, release, profile);
      assert.equal(product.output.split('|')[0], fixture.git('rev-parse', '--short=7', 'HEAD'));
      assert.notEqual(product.output, validation.output);
    } finally {
      fixture.close();
    }
  }
});
