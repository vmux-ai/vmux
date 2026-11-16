const assert = require('node:assert/strict');
const { test } = require('node:test');
const { execFileSync, spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const Preflight = require('./ci-preflight.js');
const CliArtifact = require('./ci-cli.js');
const Checks = require('./ci-checks.js');
const Sources = require('./ci-cache.js');

test('push formatting works without a prepared CLI and preserves formatter failures', (t) => {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'vmux-push-format-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const checkout = path.join(root, 'checkout with spaces');
  fs.mkdirSync(checkout);
  fs.writeFileSync(path.join(checkout, 'cargo'), '#!/bin/sh\n[ "$1" = fmt ] || exit 91\nprintf "%s\\n" "$PWD" > "$VMUX_FORMAT_LOG"\nexit "$VMUX_FORMAT_EXIT"\n', { mode: 0o755 });
  for (const status of [0, 23]) {
    const log = path.join(root, 'format.log');
    const result = spawnSync('/bin/sh', [path.join(__dirname, 'hooks/pre-push')], {
      cwd: checkout,
      env: { ...process.env, PATH: checkout, VMUX_FORMAT_LOG: log, VMUX_FORMAT_EXIT: String(status) },
      encoding: 'utf8',
      input: 'refs/heads/feature head refs/heads/feature base\n',
    });
    assert.equal(result.status, status, result.stderr);
    assert.equal(fs.readFileSync(log, 'utf8'), `${checkout}\n`);
    assert.equal(fs.existsSync(path.join(checkout, '.build-tools')), false);
  }
});

class Fixture {
  constructor(t) {
    this.root = fs.mkdtempSync(path.join(os.tmpdir(), 'vmux-preflight-'));
    t.after(() => fs.rmSync(this.root, { recursive: true, force: true }));
    this.git('init', '-b', 'main');
    this.git('config', 'user.name', 'Fixture');
    this.git('config', 'user.email', 'fixture@example.com');
    this.git('config', 'commit.gpgsign', 'false');
    for (const file of ['.github/workflows/ci.yml', '.github/actions/prepare-ci/action.yml', 'scripts/ci-preflight.js']) {
      fs.mkdirSync(path.dirname(path.join(this.root, file)), { recursive: true });
      fs.writeFileSync(path.join(this.root, file), 'fixture execution definition\n');
    }
    fs.writeFileSync(path.join(this.root, 'Cargo.toml'), '[workspace.package]\nversion = "1.0.0"\n');
    this.commit('initial');
    this.git('checkout', '-b', 'feature');
    fs.writeFileSync(path.join(this.root, 'feature.txt'), 'feature\n');
    this.head = this.commit('feature');
    this.git('checkout', 'main');
    fs.writeFileSync(path.join(this.root, 'base.txt'), 'base\n');
    this.base = this.commit('base');
    this.git('merge', '--no-ff', 'feature', '-m', 'merge');
    this.merge = this.git('rev-parse', 'HEAD');
    this.tree = this.git('rev-parse', 'HEAD^{tree}');
    this.identity = { head: this.head, base: this.base, tree: this.tree, branch: 'feature', repository: 'owner/repo' };
    this.run = { id: 42, status: 'completed', conclusion: 'success', event: 'push', head_sha: this.head, head_branch: 'feature', repository: { full_name: 'owner/repo' }, head_repository: { full_name: 'owner/repo' }, path: '.github/workflows/ci.yml', html_url: 'https://example.com/run/42' };
    this.artifacts = [
      { name: Preflight.artifact(this.identity), expired: false },
      { name: `inputs-${Preflight.artifact(this.identity)}`, expired: false },
    ];
    this.jobs = ['Preflight Lint', 'Preflight Test', 'Preflight Build macOS', 'Preflight Build iOS', 'Record preflight'].map((name) => ({ name, conclusion: 'success' }));
    this.outputs = {};
    this.polls = 0;
    this.sleeps = 0;
    this.context = { eventName: 'pull_request', repo: { owner: 'owner', repo: 'repo' }, runId: 99, sha: this.merge, ref: 'refs/pull/1/merge', payload: { repository: { default_branch: 'main' }, pull_request: { head: { sha: this.head, ref: 'feature', repo: { full_name: 'owner/repo' } }, base: { sha: this.base, ref: 'main' } } } };
    this.github = {
      rest: {
        pulls: { list: async () => ({ data: this.pulls || [] }) },
        repos: { getBranch: async ({ branch }) => { this.baseBranch = branch; return { data: { commit: { sha: this.tip || this.base } } }; } },
        actions: {
          listWorkflowRuns: async () => ({ data: { workflow_runs: this.runs || [this.run] } }),
          getWorkflowRun: async () => { this.polls++; return { data: { ...this.run, status: 'completed', conclusion: this.finish || 'success' } }; },
          listWorkflowRunArtifacts: async () => this.artifacts,
          listJobsForWorkflowRun: async () => this.jobs,
        },
      },
      paginate: async (method, args) => method(args),
    };
    this.preflight = new Preflight({ github: this.github, context: this.context, root: this.root, core: { setOutput: (name, value) => { this.outputs[name] = value; }, info() {} }, pause: async () => { this.sleeps++; } });
  }

  git(...args) {
    return execFileSync('git', args, { cwd: this.root, env: Preflight.gitEnvironment(), encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }).trim();
  }

  commit(message) {
    this.git('add', '.');
    this.git('commit', '-m', message);
    return this.git('rev-parse', 'HEAD');
  }

  synthetic(base, head) {
    const tree = this.git('merge-tree', '--write-tree', base, head).split('\n')[0];
    return this.git('commit-tree', tree, '-p', base, '-p', head, '-m', 'Synthetic PR merge');
  }

  compilerCache(watch = '') {
    fs.writeFileSync(path.join(this.root, 'Cargo.toml'), '[package]\nname = "cache-fixture"\nversion = "0.1.0"\nedition = "2024"\n');
    fs.mkdirSync(path.join(this.root, 'src'));
    fs.writeFileSync(path.join(this.root, 'src/main.rs'), 'fn main() { println!("original"); }\n');
    if (watch === 'directory') {
      fs.writeFileSync(path.join(this.root, 'src/main.rs'), 'fn main() { println!("{}", env!("CACHE_VALUE")); }\n');
      fs.writeFileSync(path.join(this.root, 'src/value.txt'), 'original');
      fs.writeFileSync(path.join(this.root, 'build.rs'), 'fn main() { let value = std::fs::read_to_string("src/value.txt").unwrap_or_default(); println!("cargo:rustc-env=CACHE_VALUE={value}"); println!("cargo:rerun-if-changed=src"); }\n');
    }
    if (watch === 'git') {
      fs.writeFileSync(path.join(this.root, 'src/main.rs'), 'fn main() { println!("{}", env!("CACHE_VALUE")); }\n');
      fs.writeFileSync(path.join(this.root, 'build.rs'), 'fn main() { let output = std::process::Command::new("git").args(["rev-parse", "HEAD"]).output().unwrap(); let value = String::from_utf8(output.stdout).unwrap(); println!("cargo:rustc-env=CACHE_VALUE={}", value.trim()); println!("cargo:rerun-if-changed=.git/HEAD"); println!("cargo:rerun-if-changed=.git/logs/HEAD"); }\n');
    }
    this.commit('compiler fixture');
    return new Sources(this.root, '.source-cache.json', 'target');
  }

  compile(target = 'target') {
    const env = Preflight.gitEnvironment();
    for (const key of ['CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTFLAGS']) delete env[key];
    env.CARGO_INCREMENTAL = '0';
    execFileSync('cargo', ['build', '--offline', '--target-dir', target], { cwd: this.root, env, stdio: 'pipe' });
    return path.join(this.root, target, 'debug/cache-fixture');
  }

  cliCache() {
    const files = {
      '.gitignore': '.build-tools/\ntarget/\n',
      'Cargo.toml': '[workspace]\nresolver = "3"\nmembers = ["crates/app/vmux_cli", "crates/support", "crates/extra", "crates/app/gui"]\n',
      'rust-toolchain.toml': '[toolchain]\nchannel = "1.95.0"\n',
      'tool-versions.env': 'NODE_VERSION=24\n',
      'crates/app/vmux_cli/Cargo.toml': '[package]\nname = "vmux_cli"\nversion = "0.1.0"\nedition = "2024"\n[features]\ndefault = ["runtime"]\nruntime = ["dep:gui"]\ndx = []\nci-syntax = ["dx"]\n[dependencies]\nsupport = { path = "../../support" }\ngui = { path = "../gui", optional = true }\n[[bin]]\nname = "vmux"\npath = "src/main.rs"\n',
      'crates/app/vmux_cli/src/main.rs': 'fn main() { println!("{}", support::value()); }\n',
      'crates/support/Cargo.toml': '[package]\nname = "support"\nversion = "0.1.0"\nedition = "2024"\n[dependencies]\nextra = { path = "../extra", optional = true }\n',
      'crates/support/src/lib.rs': 'pub fn value() -> &\'static str { "original" }\n',
      'crates/extra/Cargo.toml': '[package]\nname = "extra"\nversion = "0.1.0"\nedition = "2024"\n',
      'crates/extra/src/lib.rs': 'pub fn value() -> &\'static str { "extra" }\n',
      'crates/app/gui/Cargo.toml': '[package]\nname = "gui"\nversion = "0.1.0"\nedition = "2024"\n',
      'crates/app/gui/src/lib.rs': 'pub fn render() {}\n',
    };
    for (const [name, contents] of Object.entries(files)) {
      const file = path.join(this.root, name);
      fs.mkdirSync(path.dirname(file), { recursive: true });
      fs.writeFileSync(file, contents);
    }
    execFileSync('cargo', ['generate-lockfile', '--offline'], { cwd: this.root, stdio: 'pipe' });
    this.commit('CLI compile fixture');
    return { GITHUB_WORKSPACE: this.root, GITHUB_RUN_ID: '123', GITHUB_REPOSITORY: 'owner/repo', RUNNER_OS: 'Linux', RUNNER_ARCH: 'X64', GITHUB_OUTPUT: path.join(this.root, 'action-output'), VMUX_CI_FAST_RUST: '1' };
  }

  restoreCache(snapshot, target, save = 'true') {
    const state = path.join(this.root, `action-state-${require('node:crypto').randomUUID()}`);
    Sources.run({ GITHUB_WORKSPACE: this.root, GITHUB_STATE: state, INPUT_SNAPSHOT: snapshot, INPUT_TARGET: target, INPUT_SAVE: save });
    const post = { GITHUB_WORKSPACE: this.root, INPUT_SNAPSHOT: '', INPUT_TARGET: '', INPUT_SAVE: '' };
    for (const line of fs.readFileSync(state, 'utf8').trim().split('\n')) {
      const separator = line.indexOf('=');
      post[`STATE_${line.slice(0, separator)}`] = line.slice(separator + 1);
    }
    return post;
  }
}

test('completed branch checks are reused only for the exact base/head merge', async (t) => {
  const fixture = new Fixture(t);
  assert.equal((await fixture.preflight.plan()).reuse, '42');
  assert.equal(fixture.sleeps, 0);
  assert.equal(fixture.outputs.tree, fixture.tree);
});

test('PR-only CI verifies its merge without looking up or waiting for branch runs', async (t) => {
  const fixture = new Fixture(t);
  fixture.github.rest.actions.listWorkflowRuns = async () => { throw new Error('Branch CI must not be queried'); };
  const plan = await fixture.preflight.plan({ reuseChecks: false });
  assert.equal(plan.tree, fixture.tree);
  assert.equal(plan.base, fixture.base);
  assert.equal(plan.reuse, '');
  assert.equal(fixture.sleeps, 0);
});

test('running branch CI is joined before accepting its completed evidence', async (t) => {
  const fixture = new Fixture(t);
  fixture.run.status = 'in_progress';
  fixture.run.conclusion = null;
  assert.equal((await fixture.preflight.plan()).reuse, '42');
  assert.equal(fixture.polls, 1);
});

test('PR planning binds stale event metadata to the captured merge parents', async (t) => {
  for (const advance of [false, true]) {
    const fixture = new Fixture(t);
    fixture.context.payload.pull_request.base.sha = fixture.git('rev-parse', `${fixture.base}^`);
    if (advance) {
      fs.writeFileSync(path.join(fixture.root, 'later.txt'), 'later base change\n');
      fixture.base = fixture.commit('advance base after merge capture');
    }
    const plan = await fixture.preflight.plan();
    assert.equal(plan.base, fixture.identity.base);
    assert.equal(plan.tree, fixture.tree);
    assert.equal(plan.reuse, '42');
    assert.equal(fixture.baseBranch, 'main');
  }
});

test('running CI against another base is not joined and missing claims fall back', async (t) => {
  for (const artifacts of [[], [{ name: `inputs-${Preflight.artifact({ head: 'a'.repeat(40), base: 'b'.repeat(40), tree: 'c'.repeat(40) })}`, expired: false }]]) {
    const fixture = new Fixture(t);
    fixture.run.status = 'in_progress';
    fixture.artifacts = artifacts;
    assert.equal((await fixture.preflight.plan()).reuse, '');
    assert.equal(fixture.polls, 0);
  }
});

test('a joined failure or cancellation fails planning', async (t) => {
  for (const conclusion of ['failure', 'cancelled', 'timed_out']) {
    const fixture = new Fixture(t);
    fixture.run.status = 'queued';
    fixture.finish = conclusion;
    await assert.rejects(fixture.preflight.plan(), new RegExp(conclusion));
    assert.equal(fixture.outputs.reuse, undefined);
  }
});

test('missing, expired, stale, failed, and incomplete evidence falls back to PR checks', async (t) => {
  const mutations = [
    (f) => { f.artifacts = []; },
    (f) => { f.artifacts[0].expired = true; },
    (f) => { f.artifacts[0].name = Preflight.artifact({ ...f.identity, base: 'a'.repeat(40) }); },
    (f) => { f.artifacts[0].name = Preflight.artifact({ ...f.identity, tree: 'a'.repeat(40) }); },
    (f) => { f.run.conclusion = 'failure'; },
    (f) => { f.jobs.pop(); },
    (f) => { f.jobs[1].conclusion = 'skipped'; },
    (f) => { f.jobs.push(f.jobs[0]); },
  ];
  for (const mutate of mutations) {
    const fixture = new Fixture(t);
    mutate(fixture);
    assert.equal((await fixture.preflight.plan()).reuse, '');
  }
});

test('run provenance excludes forks, other workflows, branches, commits, and events', async (t) => {
  for (const mutation of [
    { head_repository: { full_name: 'fork/repo' } }, { repository: { full_name: 'other/repo' } },
    { path: '.github/workflows/other.yml' }, { event: 'pull_request' },
    { head_branch: 'other' }, { head_sha: 'a'.repeat(40) },
  ]) {
    const fixture = new Fixture(t);
    Object.assign(fixture.run, mutation);
    assert.equal((await fixture.preflight.plan()).reuse, '');
  }
});

test('fork PRs and version changes always execute normal PR checks', async (t) => {
  const fork = new Fixture(t);
  fork.context.payload.pull_request.head.repo.full_name = 'fork/repo';
  assert.equal((await fork.preflight.plan()).reuse, '');
  assert.equal(fork.sleeps, 0);
  const release = new Fixture(t);
  release.git('checkout', 'feature');
  fs.writeFileSync(path.join(release.root, 'Cargo.toml'), '[workspace.package]\nversion = "2.0.0"\n');
  release.head = release.commit('release');
  release.context.payload.pull_request.head.sha = release.head;
  release.git('checkout', 'main');
  release.git('merge', '--no-ff', 'feature', '-m', 'release merge');
  release.context.sha = release.git('rev-parse', 'HEAD');
  release.tip = release.context.sha;
  const plan = await release.preflight.plan();
  assert.equal(plan.release, true);
  assert.equal(plan.reuse, '');
  assert.equal(release.outputs.receipt, '');
});

test('matching Git trees cannot reuse a run executed with an older CI definition', async (t) => {
  for (const file of ['.github/workflows/ci.yml', '.github/actions/prepare-ci/action.yml', 'scripts/ci-preflight.js']) {
    const fixture = new Fixture(t);
    fs.writeFileSync(path.join(fixture.root, file), 'updated base execution definition\n');
    fixture.base = fixture.commit('update base CI');
    fixture.tree = fixture.git('rev-parse', 'HEAD^{tree}');
    fixture.context.payload.pull_request.base.sha = fixture.base;
    fixture.context.sha = fixture.synthetic(fixture.base, fixture.head);
    fixture.artifacts = [{ name: Preflight.artifact({ ...fixture.identity, base: fixture.base, tree: fixture.tree }), expired: false }];
    assert.equal((await fixture.preflight.plan()).reuse, '');
    assert.equal(fixture.outputs.reusable, 'false');
    assert.equal(fixture.outputs.receipt, '');
  }
});

test('a push plans against an existing stacked PR base', async (t) => {
  const fixture = new Fixture(t);
  fixture.context.eventName = 'push';
  fixture.context.sha = fixture.head;
  fixture.context.ref = 'refs/heads/feature';
  delete fixture.context.payload.pull_request;
  fixture.pulls = [{ base: { ref: 'stack' } }];
  const plan = await fixture.preflight.plan();
  assert.equal(plan.feature, true);
  assert.equal(fixture.baseBranch, 'stack');
  assert.equal(plan.tree, fixture.tree);
  assert.equal(plan.reuse, '');
});

test('main, schedules, and manual CI retain normal execution', async (t) => {
  for (const eventName of ['push', 'schedule', 'workflow_dispatch']) {
    const fixture = new Fixture(t);
    fixture.context.eventName = eventName;
    fixture.context.ref = 'refs/heads/main';
    delete fixture.context.payload.pull_request;
    const plan = await fixture.preflight.plan();
    assert.equal(plan.feature, false);
    assert.equal(plan.base, '');
    assert.equal(plan.reuse, '');
  }
});

test('unexpected PR merge trees fail instead of reusing evidence', async (t) => {
  const fixture = new Fixture(t);
  const invalid = fixture.git('commit-tree', fixture.git('rev-parse', `${fixture.head}^{tree}`), '-p', fixture.base, '-p', fixture.head, '-m', 'invalid merge tree');
  for (const sha of [fixture.head, invalid]) {
    fixture.context.sha = sha;
    await assert.rejects(fixture.preflight.plan(), /does not match/);
  }
});

test('PR merges capture an advanced target base without reusing older branch evidence', async (t) => {
  const fixture = new Fixture(t);
  fixture.git('checkout', '--detach', fixture.base);
  fs.writeFileSync(path.join(fixture.root, 'advanced.txt'), 'Advanced main\n');
  fixture.base = fixture.commit('Advance target branch');
  fixture.context.sha = fixture.synthetic(fixture.base, fixture.head);
  const plan = await fixture.preflight.plan();
  assert.equal(plan.base, fixture.base);
  assert.equal(plan.tree, fixture.git('rev-parse', `${fixture.context.sha}^{tree}`));
  assert.equal(plan.reuse, '');
  assert.equal(fixture.baseBranch, 'main');
});

test('PR merge inputs reject reversed parents and bases outside the target branch', async (t) => {
  const reversed = new Fixture(t);
  reversed.context.sha = reversed.synthetic(reversed.head, reversed.base);
  await assert.rejects(reversed.preflight.plan(), /does not match/);
  const unrelated = new Fixture(t);
  unrelated.git('checkout', '--detach', unrelated.base);
  fs.writeFileSync(path.join(unrelated.root, 'other.txt'), 'Other branch\n');
  const base = unrelated.commit('Commit outside target branch');
  unrelated.context.sha = unrelated.synthetic(base, unrelated.head);
  await assert.rejects(unrelated.preflight.plan(), /target branch ancestry/);
  const wrongPayload = new Fixture(t);
  wrongPayload.context.payload.pull_request.base.sha = wrongPayload.head;
  await assert.rejects(wrongPayload.preflight.plan(), /target branch ancestry/);
});

test('prepared checkouts compile the merged code from both branches', (t) => {
  const fixture = new Fixture(t);
  fixture.git('checkout', '--detach', fixture.head);
  assert.equal(fs.existsSync(path.join(fixture.root, 'base.txt')), false);
  const env = { VMUX_CI_HEAD: fixture.head, VMUX_CI_BASE: fixture.base, VMUX_CI_TREE: fixture.tree };
  const commit = Preflight.checkout(env, fixture.root);
  assert.equal(commit, fixture.git('rev-parse', 'HEAD'));
  assert.equal(fixture.git('rev-parse', 'HEAD^{tree}'), fixture.tree);
  assert.equal(fs.readFileSync(path.join(fixture.root, 'base.txt'), 'utf8'), 'base\n');
  assert.equal(fs.readFileSync(path.join(fixture.root, 'feature.txt'), 'utf8'), 'feature\n');
  assert.deepEqual(fixture.git('diff', '--name-only', fixture.base, 'HEAD').split('\n'), ['feature.txt']);
  assert.equal(fixture.git('status', '--porcelain'), '');
  assert.throws(() => Preflight.checkout({ ...env, VMUX_CI_TREE: 'a'.repeat(40) }, fixture.root), /changed/);
  assert.throws(() => Preflight.checkout({ ...env, VMUX_CI_BASE: '--help' }, fixture.root), /Invalid/);
});

test('Git hook repository variables cannot redirect an explicitly selected checkout', (t) => {
  const fixture = new Fixture(t);
  const other = new Fixture(t);
  const names = ['GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE'];
  const previous = names.map((name) => process.env[name]);
  try {
    process.env.GIT_DIR = path.join(other.root, '.git');
    process.env.GIT_WORK_TREE = other.root;
    process.env.GIT_INDEX_FILE = path.join(other.root, '.git/index');
    new Fixture(t);
    Preflight.checkout({ VMUX_CI_HEAD: fixture.head, VMUX_CI_BASE: fixture.base, VMUX_CI_TREE: fixture.tree }, fixture.root);
    assert.equal(fixture.git('rev-parse', 'HEAD^{tree}'), fixture.tree);
    assert.equal(other.git('rev-parse', 'HEAD'), other.merge);
    assert.equal(other.git('status', '--porcelain'), '');
    assert.equal(other.git('config', '--get', 'core.bare'), 'false');
  } finally {
    for (let index = 0; index < names.length; index++) {
      if (previous[index] === undefined) delete process.env[names[index]];
      else process.env[names[index]] = previous[index];
    }
  }
});

test('conflicting branch merges stop CI planning', async (t) => {
  const fixture = new Fixture(t);
  fixture.git('checkout', 'feature');
  fs.writeFileSync(path.join(fixture.root, 'Cargo.toml'), '[workspace.package]\nversion = "2.0.0"\n');
  fixture.head = fixture.commit('head version');
  fixture.git('checkout', 'main');
  fs.writeFileSync(path.join(fixture.root, 'Cargo.toml'), '[workspace.package]\nversion = "3.0.0"\n');
  fixture.base = fixture.commit('base version');
  fixture.context.payload.pull_request.head.sha = fixture.head;
  fixture.context.payload.pull_request.base.sha = fixture.base;
  await assert.rejects(fixture.preflight.plan());
  assert.equal(fixture.outputs.reuse, undefined);
});

test('waiting is bounded and API errors cannot grant reuse', async (t) => {
  const fixture = new Fixture(t);
  fixture.run.status = 'in_progress';
  let clock = 0;
  fixture.preflight.now = () => { clock += 90 * 60 * 1000; return clock; };
  await assert.rejects(fixture.preflight.plan(), /85 minutes/);
  assert.equal(fixture.outputs.reuse, undefined);
  fixture.github.rest.actions.listWorkflowRuns = async () => { throw new Error('API unavailable'); };
  await assert.rejects(fixture.preflight.plan(), /API unavailable/);
});

test('recording requires every applicable result and exact input identity', (t) => {
  const fixture = new Fixture(t);
  const results = {
    plan: { result: 'success', outputs: { ...fixture.identity, feature: 'true', release: 'false', reusable: 'true', reuse: '', receipt: Preflight.artifact(fixture.identity) } },
    changes: { result: 'success', outputs: { website: 'false' } },
    lint: { result: 'success' }, test: { result: 'success' },
    'build-macos': { result: 'success' }, 'build-ios': { result: 'success' }, website: { result: 'skipped' },
  };
  const record = (value) => Preflight.record({ VMUX_CI_RESULTS: JSON.stringify(value), GITHUB_RUN_ID: '42' }, fixture.root);
  record(results);
  assert.equal(JSON.parse(fs.readFileSync(path.join(fixture.root, 'ci-preflight.json'), 'utf8')).run, '42');
  for (const name of ['plan', 'changes', 'lint', 'test', 'build-macos', 'build-ios']) {
    for (const result of ['failure', 'cancelled', 'skipped']) {
      const invalid = structuredClone(results);
      invalid[name].result = result;
      assert.throws(() => record(invalid), /Cannot record/);
    }
  }
  for (const mutation of [
    (r) => { r.changes.outputs.website = 'true'; },
    (r) => { r.plan.outputs.reuse = '42'; },
    (r) => { r.plan.outputs.feature = 'false'; },
    (r) => { r.plan.outputs.release = 'true'; },
    (r) => { r.plan.outputs.reusable = 'false'; },
    (r) => { r.plan.outputs.receipt = 'other'; },
  ]) {
    const invalid = structuredClone(results);
    mutation(invalid);
    assert.throws(() => record(invalid));
  }
  results.changes.outputs.website = 'true';
  results.website.result = 'success';
  record(results);
});

class Workflow {
  constructor() {
    const file = path.join(__dirname, '../.github/workflows/ci.yml');
    const document = JSON.parse(execFileSync('ruby', ['-rjson', '-ryaml', '-e', 'doc = YAML.load_file(ARGV[0]); puts JSON.generate({"on" => doc["on"] || doc[true], "jobs" => doc["jobs"]})', file], { encoding: 'utf8' }));
    this.events = document.on;
    this.jobs = document.jobs;
    const filter = this.jobs.changes.steps.find((step) => step.id === 'filter');
    this.filters = JSON.parse(execFileSync('ruby', ['-rjson', '-ryaml', '-e', 'puts JSON.generate(YAML.load(STDIN.read))'], { input: filter.with.filters, encoding: 'utf8' }));
  }

  triggered(event, branch, files) {
    if (!(event in this.events)) return false;
    if (event !== 'push') return true;
    const push = this.events.push;
    if (!push.branches.some((pattern) => path.matchesGlob(branch, pattern))) return false;
    return files.some((file) => !push['paths-ignore'].some((pattern) => path.matchesGlob(file, pattern)));
  }

  async checkProfiles(identity, context, inputs = {}) {
    const captured = {};
    class Adapter {
      async plan(_identity, profiles) { captured.profiles = profiles; }
    }
    const step = this.jobs.changes.steps.find((step) => step.id === 'checks');
    const AsyncFunction = Object.getPrototypeOf(async function() {}).constructor;
    const execute = new AsyncFunction('require', 'github', 'context', 'core', 'process', step.with.script);
    await execute(() => Adapter, {}, context, {}, { env: { VMUX_CHECK_BASE: identity.base, VMUX_CHECK_TREE: identity.tree, VMUX_CHECK_RELEASE: 'false', VMUX_LINT_SCOPE: '-p example', VMUX_TEST_SCOPE: '-p example', VMUX_CHECK_INPUTS: JSON.stringify(inputs), VMUX_RECORDS: 'false', RUNNER_OS: 'Linux', RUNNER_ARCH: 'X64', ImageVersion: 'fixture' } });
    return captured.profiles;
  }

  gate(name, env) {
    return require('node:child_process').spawnSync('bash', ['-c', this.jobs[name].steps[0].run], { env: { ...process.env, ...env }, encoding: 'utf8' });
  }

  step(name, title, event, feature, changes) {
    const step = this.jobs[name].steps.find((step) => step.name === title);
    return this.lane(name, event, feature, changes, '', step.if);
  }

  lane(name, event, feature, changes, reuse = '', condition = this.jobs[name].if, release = 'false') {
    const github = { event_name: event };
    const outputs = {};
    for (const [key, value] of Object.entries(changes)) outputs[key] = String(value);
    const needs = { plan: { outputs: { feature, reuse, release } }, changes: { outputs } };
    return Function('github', 'needs', `return (${condition});`)(github, needs);
  }

  ready(name, results) {
    return this.jobs[name].needs.every((dependency) => results[dependency] === 'success');
  }

  paths(files) {
    const outputs = {};
    for (const [name, patterns] of Object.entries(this.filters)) {
      const include = patterns.filter((pattern) => !pattern.startsWith('!'));
      const exclude = patterns.filter((pattern) => pattern.startsWith('!'));
      outputs[name] = String(files.some((file) => include.some((pattern) => path.matchesGlob(file, pattern)) && !exclude.some((pattern) => path.matchesGlob(file, pattern.slice(1)))));
    }
    return outputs;
  }

  filterRef(base, commit) {
    const filter = this.jobs.changes.steps.find((step) => step.uses === 'dorny/paths-filter@v4');
    return Function('needs', 'steps', `return (${filter.with.ref.slice(3, -2)});`)({ plan: { outputs: { base } } }, { prepared: { outputs: { commit } } });
  }

  changes(event, feature, base, native, filter = { macos: 'true', macos_package: 'true', macos_tooling: 'true', ios: 'true', code: 'true' }, lean = 'true', release = 'false', selection = {}) {
    const github = { event_name: event };
    const needs = { plan: { outputs: { feature, base, release } } };
    const steps = {
      filter: { outputs: filter },
      native: { outputs: { native } },
      scope: { outputs: { lean, sdk: "true", headless: "true", rust: "true", records: "true", render_free: "true", tooling: "true", ...selection } },
      checks: { outputs: {} },
    };
    for (const name of Object.keys(this.jobs.changes.outputs)) {
      if (name.startsWith('check_') || name.startsWith('reuse_')) steps.checks.outputs[name] = '';
    }
    const classify = this.jobs.changes.steps.find((step) => step.id === 'native');
    if (!Function('github', 'needs', 'steps', `return (${classify.if});`)(github, needs, steps)) steps.native.outputs.native = '';
    const result = {};
    for (const [key, value] of Object.entries(this.jobs.changes.outputs)) {
      result[key] = Function('github', 'needs', 'steps', `return (${value.slice(3, -2)});`)(github, needs, steps);
    }
    return result;
  }
}

test('development CLI artifacts bind executable bytes to the exact run, source and platform', (t) => {
  const fixture = new Fixture(t);
  fs.writeFileSync(path.join(fixture.root, 'rust-toolchain.toml'), '[toolchain]\nchannel = "1.95.0"\n');
  fixture.commit('toolchain');
  const env = { GITHUB_RUN_ID: '123', GITHUB_REPOSITORY: 'owner/repo', RUNNER_OS: 'Linux', RUNNER_ARCH: 'X64', VMUX_CI_FAST_RUST: '1' };
  const binary = path.join(fixture.root, 'cli-input');
  const directory = path.join(fixture.root, 'artifact');
  fs.writeFileSync(binary, '#!/bin/sh\nexit 0\n');
  CliArtifact.publish(binary, directory, env, fixture.root);
  const manifestPath = path.join(directory, 'manifest.json');
  const original = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  const installed = path.join(fixture.root, '.build-tools/bin/vmux');
  CliArtifact.install(directory, env, fixture.root);
  assert.equal(fs.readFileSync(installed, 'utf8'), fs.readFileSync(binary, 'utf8'));
  assert.equal(fs.statSync(installed).mode & 0o111, 0o111);
  for (const [key, value] of Object.entries({ run: '124', repository: 'other/repo', tree: 'a'.repeat(40), toolchain: '1.94.0', os: 'macOS', arch: 'ARM64', features: 'runtime', compile: 'default' })) {
    fs.writeFileSync(manifestPath, JSON.stringify({ ...original, [key]: value }));
    assert.throws(() => CliArtifact.install(directory, env, fixture.root), /does not match/, key);
  }
  fs.writeFileSync(manifestPath, JSON.stringify(original));
  fs.unlinkSync(installed);
  fs.symlinkSync(binary, installed);
  assert.throws(() => CliArtifact.install(directory, env, fixture.root), /destination/);
  assert.equal(fs.readFileSync(binary, 'utf8'), '#!/bin/sh\nexit 0\n');
  fs.unlinkSync(installed);
  fs.symlinkSync(path.join(fixture.root, 'missing'), installed);
  assert.throws(() => CliArtifact.install(directory, env, fixture.root), /destination/);
  assert.equal(fs.existsSync(path.join(fixture.root, 'missing')), false);
  fs.unlinkSync(installed);
  CliArtifact.install(directory, env, fixture.root);
  fs.appendFileSync(path.join(directory, 'vmux'), 'changed');
  assert.throws(() => CliArtifact.install(directory, env, fixture.root), /checksum/);
  assert.equal(fs.readFileSync(installed, 'utf8'), fs.readFileSync(binary, 'utf8'));
  fs.unlinkSync(path.join(directory, 'vmux'));
  fs.symlinkSync(binary, path.join(directory, 'vmux'));
  assert.throws(() => CliArtifact.install(directory, env, fixture.root), /Invalid/);
});

test('unchanged CLI compile inputs reuse executable bytes across runs and unrelated app edits', (t) => {
  const fixture = new Fixture(t);
  const env = fixture.cliCache();
  const directory = path.join(fixture.root, '.build-tools/cli-cache');
  CliArtifact.prepareCache(env, fixture.root);
  execFileSync('cargo', ['build', '--offline', '-p', 'vmux_cli', '--no-default-features', '--features', 'dx,ci-syntax'], { cwd: fixture.root, stdio: 'pipe' });
  CliArtifact.installBinary(path.join(fixture.root, 'target/debug/vmux'), fixture.root);
  CliArtifact.publishCache(directory, env, fixture.root);
  fs.writeFileSync(path.join(fixture.root, 'crates/app/gui/src/lib.rs'), 'pub fn render() { panic!("unrelated"); }\n');
  fs.writeFileSync(path.join(fixture.root, 'readme.md'), 'unrelated documentation\n');
  fixture.commit('unrelated inputs');
  env.GITHUB_RUN_ID = '124';
  CliArtifact.prepareCache(env, fixture.root);
  fs.rmSync(path.join(fixture.root, 'target'), { recursive: true });
  fs.rmSync(path.join(fixture.root, '.build-tools/bin'), { recursive: true });
  assert.equal(CliArtifact.reuseCache(directory, env, fixture.root), true);
  const binary = path.join(fixture.root, '.build-tools/bin/vmux');
  assert.equal(execFileSync(binary, { encoding: 'utf8' }), 'original\n');
  const artifact = path.join(fixture.root, '.build-tools/current-run');
  CliArtifact.publish(binary, artifact, env, fixture.root);
  assert.throws(() => CliArtifact.install(artifact, { ...env, GITHUB_RUN_ID: '123' }, fixture.root), /run does not match/);
  CliArtifact.install(artifact, env, fixture.root);
});

test('CLI reuse rejects changed dependencies, modes, tool inputs, platform, flags and corrupt bytes', (t) => {
  const fixture = new Fixture(t);
  const env = fixture.cliCache();
  const directory = path.join(fixture.root, '.build-tools/cli-cache');
  const source = path.join(fixture.root, 'crates/support/src/lib.rs');
  CliArtifact.prepareCache(env, fixture.root);
  const binary = path.join(fixture.root, '.build-tools/bin/vmux');
  fs.mkdirSync(path.dirname(binary), { recursive: true });
  fs.writeFileSync(binary, '#!/bin/sh\necho original\n');
  CliArtifact.publishCache(directory, env, fixture.root);
  for (const [key, value] of Object.entries({ GITHUB_REPOSITORY: 'other/repo', RUNNER_OS: 'macOS', RUNNER_ARCH: 'ARM64', VMUX_CI_FAST_RUST: '0', CARGO_PROFILE_DEV_LTO: 'off' })) {
    CliArtifact.prepareCache({ ...env, [key]: value }, fixture.root);
    assert.equal(CliArtifact.reuseCache(directory, env, fixture.root), false, key);
  }
  for (const name of ['crates/support/src/lib.rs', 'tool-versions.env', 'Cargo.lock']) {
    const file = path.join(fixture.root, name);
    const original = fs.readFileSync(file);
    fs.appendFileSync(file, '\n');
    CliArtifact.prepareCache(env, fixture.root);
    assert.equal(CliArtifact.reuseCache(directory, env, fixture.root), false, name);
    fs.writeFileSync(file, original);
  }
  fs.chmodSync(source, 0o755);
  CliArtifact.prepareCache(env, fixture.root);
  assert.equal(CliArtifact.reuseCache(directory, env, fixture.root), false);
  fs.chmodSync(source, 0o644);
  CliArtifact.prepareCache(env, fixture.root);
  fs.appendFileSync(path.join(directory, 'vmux'), 'corrupt');
  assert.equal(CliArtifact.reuseCache(directory, env, fixture.root), false);
  fs.unlinkSync(path.join(directory, 'vmux'));
  fs.symlinkSync(binary, path.join(directory, 'vmux'));
  assert.equal(CliArtifact.reuseCache(directory, env, fixture.root), false);
  const before = fs.readFileSync(binary);
  CliArtifact.publishCache(directory, env, fixture.root);
  assert.equal(fs.lstatSync(path.join(directory, 'vmux')).isSymbolicLink(), false);
  assert.deepEqual(fs.readFileSync(binary), before);
  assert.equal(CliArtifact.reuseCache(directory, env, fixture.root), true);
});

test('changed embedded update keys rebuild the CLI instead of reusing an old key', (t) => {
  const fixture = new Fixture(t);
  const env = { ...fixture.cliCache(), VMUX_UPDATE_PUBLIC_KEY: 'key-a' };
  const source = path.join(fixture.root, 'crates/app/vmux_cli/src/main.rs');
  fs.writeFileSync(source, 'fn main() { println!("{}", option_env!("VMUX_UPDATE_PUBLIC_KEY").unwrap_or("unset")); }\n');
  fs.writeFileSync(path.join(fixture.root, 'crates/app/vmux_cli/build.rs'), 'fn main() { println!("cargo:rerun-if-env-changed=VMUX_UPDATE_PUBLIC_KEY"); }\n');
  fixture.commit('embedded CLI key fixture');
  const directory = path.join(fixture.root, '.build-tools/cli-cache');
  const binary = path.join(fixture.root, 'target/debug/vmux');
  for (const key of ['key-a', 'key-b']) {
    env.VMUX_UPDATE_PUBLIC_KEY = key;
    CliArtifact.prepareCache(env, fixture.root);
    if (key === 'key-b') assert.equal(CliArtifact.reuseCache(directory, env, fixture.root), false);
    execFileSync('cargo', ['build', '--offline', '-p', 'vmux_cli', '--no-default-features', '--features', 'dx,ci-syntax'], { cwd: fixture.root, env: { ...process.env, VMUX_UPDATE_PUBLIC_KEY: key }, stdio: 'pipe' });
    CliArtifact.installBinary(binary, fixture.root);
    assert.equal(execFileSync(binary, { encoding: 'utf8' }), `${key}\n`);
    CliArtifact.publishCache(directory, env, fixture.root);
  }
});

test('deleting a compile input cannot be hidden inside another file payload', (t) => {
  const fixture = new Fixture(t);
  const env = fixture.cliCache();
  const first = path.join(fixture.root, 'crates/support/a.bin');
  const secondName = 'crates/support/b.bin';
  const second = path.join(fixture.root, secondName);
  fs.writeFileSync(first, 'original');
  fs.writeFileSync(second, 'additional');
  fs.writeFileSync(path.join(fixture.root, 'crates/app/vmux_cli/src/main.rs'), 'fn main() { println!("{}", include_bytes!("../../../support/a.bin").len()); }\n');
  fixture.commit('binary compiler inputs');
  CliArtifact.prepareCache(env, fixture.root);
  execFileSync('cargo', ['build', '--offline', '-p', 'vmux_cli', '--no-default-features', '--features', 'dx,ci-syntax'], { cwd: fixture.root, stdio: 'pipe' });
  const binary = path.join(fixture.root, 'target/debug/vmux');
  const original = execFileSync(binary, { encoding: 'utf8' });
  const directory = path.join(fixture.root, '.build-tools/cli-cache');
  CliArtifact.installBinary(binary, fixture.root);
  CliArtifact.publishCache(directory, env, fixture.root);
  const header = Buffer.from(`${secondName}\0${fs.statSync(second).mode}\0`);
  fs.appendFileSync(first, Buffer.concat([header, fs.readFileSync(second)]));
  fs.unlinkSync(second);
  fixture.commit('delete and fold compiler input');
  CliArtifact.prepareCache(env, fixture.root);
  assert.equal(CliArtifact.reuseCache(directory, env, fixture.root), false);
  execFileSync('cargo', ['build', '--offline', '-p', 'vmux_cli', '--no-default-features', '--features', 'dx,ci-syntax'], { cwd: fixture.root, stdio: 'pipe' });
  assert.notEqual(execFileSync(binary, { encoding: 'utf8' }), original);
});

test('newly enabled local dependencies join the CLI input closure', (t) => {
  const fixture = new Fixture(t);
  const env = fixture.cliCache();
  const manifest = path.join(fixture.root, 'crates/support/Cargo.toml');
  fs.appendFileSync(manifest, '[features]\ndefault = ["dep:extra"]\n');
  fixture.commit('enable new CLI dependency');
  const directory = path.join(fixture.root, '.build-tools/cli-cache');
  CliArtifact.prepareCache(env, fixture.root);
  const binary = path.join(fixture.root, '.build-tools/bin/vmux');
  fs.mkdirSync(path.dirname(binary), { recursive: true });
  fs.writeFileSync(binary, '#!/bin/sh\necho extra\n');
  CliArtifact.publishCache(directory, env, fixture.root);
  fs.writeFileSync(path.join(fixture.root, 'crates/extra/src/lib.rs'), 'pub fn value() -> &\'static str { "changed" }\n');
  CliArtifact.prepareCache(env, fixture.root);
  assert.equal(CliArtifact.reuseCache(directory, env, fixture.root), false);
});

test('CI-only Rust settings preserve existing flags and encoded-flag precedence', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vmux-rust-settings-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const [index, encoded] of [undefined, '', '--cfg\x1fexisting'].entries()) {
    const file = path.join(root, String(index));
    const env = { GITHUB_ENV: file, RUSTFLAGS: '--cfg existing', CARGO_ENCODED_RUSTFLAGS: encoded };
    CliArtifact.configureFastRust(env);
    const output = fs.readFileSync(file, 'utf8');
    assert.match(output, /CARGO_PROFILE_DEV_LTO=off/);
    if (encoded !== undefined) {
      const flags = output.split('\n').find((line) => line.startsWith('CARGO_ENCODED_RUSTFLAGS=')).slice('CARGO_ENCODED_RUSTFLAGS='.length).split('\x1f');
      assert.deepEqual(flags, encoded === '' ? ['-C', 'opt-level=1', '-C', 'codegen-units=16'] : ['--cfg', 'existing', '-C', 'opt-level=1', '-C', 'codegen-units=16']);
      assert.equal(output.includes('\nRUSTFLAGS='), false);
    } else assert.ok(output.includes('RUSTFLAGS=--cfg existing -C opt-level=1 -C codegen-units=16'));
  }
  assert.throws(() => CliArtifact.configureFastRust({ GITHUB_ENV: path.join(root, 'invalid'), RUSTFLAGS: 'value\nINJECTED=1' }), /multiline/);
  assert.equal(fs.existsSync(path.join(root, 'invalid')), false);
});

test('change detection receives a local merge object ID accepted by paths-filter', (t) => {
  const fixture = new Fixture(t);
  const commit = Preflight.checkout({ VMUX_CI_HEAD: fixture.head, VMUX_CI_BASE: fixture.base, VMUX_CI_TREE: fixture.tree }, fixture.root);
  const workflow = new Workflow();
  const ref = workflow.filterRef(fixture.base, commit);
  assert.match(ref, /^[a-f0-9]{40}$/);
  assert.equal(fixture.git('cat-file', '-t', ref), 'commit');
  assert.deepEqual(fixture.git('diff', '--name-only', fixture.base, ref).split('\n'), ['feature.txt']);
  assert.equal(workflow.filterRef('', commit), '');
});

test('ordinary PRs never select macOS or iOS builds, including packaging changes', () => {
  const workflow = new Workflow();
  for (const file of ['.github/workflows/ci.yml', 'crates/app/vmux_desktop/src/main.rs', 'crates/app/vmux_mobile/src/main.rs', 'packaging/Info.plist', 'Cargo.toml']) {
    const changes = workflow.changes('pull_request', 'false', 'base', 'true', workflow.paths([file]));
    for (const name of ['dx-macos', 'build-mac', 'ios-build']) {
      assert.equal(workflow.lane(name, 'pull_request', 'false', changes), false, `${file}: ${name}`);
    }
  }
});

test('feature pushes do not dispatch CI while PRs, main, and explicit validation do', () => {
  const workflow = new Workflow();
  for (const [event, branch, file, triggered] of [
    ['push', 'feature/example', 'crates/feature/vmux_terminal/src/lib.rs', false],
    ['push', 'main', 'crates/feature/vmux_terminal/src/lib.rs', true],
    ['push', 'main', 'Casks/vmux.rb', false],
    ['pull_request', 'feature/example', 'crates/feature/vmux_terminal/src/lib.rs', true],
    ['workflow_dispatch', 'feature/example', 'crates/feature/vmux_terminal/src/lib.rs', true],
    ['schedule', 'main', 'crates/feature/vmux_terminal/src/lib.rs', true],
  ]) {
    assert.equal(workflow.triggered(event, branch, [file]), triggered, `${event}: ${branch}: ${file}`);
  }
});

test('main and explicit full validation retain native builds and release PRs retain packaging', () => {
  const workflow = new Workflow();
  for (const event of ['push', 'workflow_dispatch']) {
    const changes = workflow.changes(event, 'false', '', '', workflow.paths(['Cargo.toml']));
    for (const name of ['dx-macos', 'build-mac', 'ios-build']) {
      assert.equal(workflow.lane(name, event, 'false', changes), true, `${event}: ${name}`);
    }
  }
  for (const native of ['true', 'false']) {
    const changes = workflow.changes('pull_request', 'false', 'base', native, workflow.paths(['Cargo.toml']), 'true', 'true');
    assert.equal(changes.macos, native === 'true');
    assert.equal(changes.macos_tooling, native === 'true');
    assert.equal(changes.ios, false);
    for (const name of ['dx-macos', 'build-mac']) {
      assert.equal(workflow.lane(name, 'pull_request', 'false', changes, '', workflow.jobs[name].if, 'true'), native === 'true', `${native}: ${name}`);
    }
  }
  const scheduled = workflow.changes('schedule', 'false', '', '', workflow.paths([]));
  assert.equal(workflow.lane('ios-build', 'schedule', 'false', scheduled), true);
});

test('immutable Git SDK consumer validation stays outside ordinary PRs', () => {
  const workflow = new Workflow();
  for (const event of ['pull_request', 'push', 'schedule', 'workflow_dispatch']) {
    assert.equal(workflow.step('test-sdk', 'Test immutable Git dependencies', event, 'false', {}), event !== 'pull_request', event);
  }
});

test('feature variants remain parallel with ordinary tests and required when selected', () => {
  const workflow = new Workflow();
  for (const name of ['test-sdk', 'test-headless', 'test-lean']) {
    assert.equal(workflow.ready(name, { plan: 'success', changes: 'success', 'test-rust': 'in_progress' }), true);
  }
  assert.equal(workflow.lane('test-lean', 'pull_request', 'false', { code: 'true', contracts: 'false', lean: 'true' }), true);
  assert.equal(workflow.lane('test-lean', 'pull_request', 'false', { code: 'true', contracts: 'false', lean: 'false' }), false);
  const valid = { PLAN: 'success', CHANGES: 'success', CODE: 'true', RUST_REQUIRED: 'true', HEADLESS_REQUIRED: 'true', CONTRACTS: 'false', LEAN: 'true', RUST_TEST: 'success', SDK: 'success', HEADLESS: 'success', LEAN_TEST: 'success' };
  assert.equal(workflow.gate('test', valid).status, 0);
  for (const result of ['failure', 'cancelled', 'skipped']) {
    assert.equal(workflow.gate('test', { ...valid, LEAN_TEST: result }).status, 1, result);
  }
});

test('required statuses fail on preparation errors and applicable child failures or unexpected skips', () => {
  const workflow = new Workflow();
  const valid = { PLAN: 'success', CHANGES: 'success', CODE: 'true', RUST_REQUIRED: 'true', HEADLESS_REQUIRED: 'true', CONTRACTS: 'true', LEAN: 'false', RUST_TEST: 'success', SDK: 'success', HEADLESS: 'success', LEAN_TEST: 'success', DISTRIBUTION: 'true', MACOS: 'true', RESULT: 'success' };
  for (const name of ['lint', 'test', 'build-macos']) {
    assert.equal(workflow.gate(name, valid).status, 0, name);
    for (const key of ['PLAN', 'CHANGES']) {
      for (const result of ['failure', 'cancelled', 'skipped']) {
        assert.equal(workflow.gate(name, { ...valid, [key]: result }).status, 1, `${name}: ${key}: ${result}`);
      }
    }
  }
  for (const result of ['failure', 'cancelled', 'skipped']) {
    for (const key of ['RUST_TEST', 'SDK', 'HEADLESS']) {
      assert.equal(workflow.gate('test', { ...valid, [key]: result }).status, 1, `${key}: ${result}`);
    }
    assert.equal(workflow.gate('build-macos', { ...valid, RESULT: result }).status, 1);
  }
  assert.equal(workflow.gate('test', { ...valid, CODE: 'false', RUST_TEST: 'skipped', SDK: 'skipped', HEADLESS: 'skipped', LEAN_TEST: 'skipped' }).status, 0);
  assert.equal(workflow.gate('build-macos', { ...valid, DISTRIBUTION: 'false', RESULT: 'skipped' }).status, 0);
});

test('compiler-cache policy pairs each workflow restore with a successful explicit save', () => {
  const files = fs.readdirSync(path.join(__dirname, '../.github/workflows')).filter((name) => name.endsWith('.yml'));
  for (const file of files) {
    const workflow = JSON.parse(execFileSync('ruby', ['-ryaml', '-rjson', '-e', 'puts JSON.generate(YAML.load_file(ARGV[0]))', path.join(__dirname, '../.github/workflows', file)], { encoding: 'utf8' }));
    for (const [name, job] of Object.entries(workflow.jobs)) {
      const steps = job.steps || [];
      const setup = steps.filter((step) => ['./.github/actions/setup-linux', './.github/actions/setup-macos', './.github/actions/setup-ios', './.github/actions/setup-web'].includes(step.uses));
      const saves = steps.filter((step) => step.uses === './.github/actions/save-workspace');
      assert.deepEqual(saves.map((step) => step.with['cache-key']).sort(), setup.map((step) => step.with['cache-key']).sort(), `${file}: ${name}`);
      for (const restore of setup) {
        const save = saves.find((step) => step.with['cache-key'] === restore.with['cache-key']);
        assert.ok(steps.indexOf(save) > steps.indexOf(restore));
        assert.equal(save.if, restore.if, `${file}: ${name}: save must require successful applicable preparation`);
      }
    }
  }
});

test('superseded PRs cancel heavy Lint while failed preparation still runs its gate', () => {
  const workflow = new Workflow();
  const expression = workflow.jobs.lint.if;
  const body = expression.startsWith('${{') ? expression.slice(3, -2) : expression;
  for (const state of ['success', 'failure', 'cancelled']) {
    const applicable = Function('always', 'cancelled', `return (${body});`)(() => true, () => state === 'cancelled');
    assert.equal(applicable, state !== 'cancelled', state);
  }
});

test('required lint runs compiler fixtures and source checks unless identical inputs were verified', () => {
  const workflow = new Workflow();
  for (const name of ['Check typed registration fixtures', 'Lint registered systems']) {
    for (const [code, reuse_lint, selected] of [['true', '', true], ['false', '', false], ['true', '42', false]]) {
      assert.equal(workflow.step('lint', name, 'pull_request', 'false', { code, reuse_lint, lint_args: '-p vmux_dx' }), selected, `${name}: ${code}: ${reuse_lint}`);
    }
    const step = workflow.jobs.lint.steps.find((step) => step.name === name);
    assert.equal(step['continue-on-error'], undefined);
  }
  assert.equal(workflow.step('lint', 'Lint registered systems', 'pull_request', 'false', { code: 'true', lint_args: '' }), false);
  assert.equal(workflow.step('lint', 'Check typed registration fixtures', 'pull_request', 'false', { code: 'true', lint_args: '' }), true);
});

test('post-job snapshots preserve inputs when Actions clears action inputs', (t) => {
  for (const save of ['true', 'false']) {
    const fixture = new Fixture(t);
    const snapshot = path.join(fixture.root, '.source-cache.json');
    const post = fixture.restoreCache('.source-cache.json', 'target', save);
    fs.writeFileSync(path.join(fixture.root, 'feature.txt'), 'final job contents\n');
    Sources.run(post);
    assert.equal(fs.existsSync(snapshot), save === 'true');
    if (save === 'true') {
      const record = JSON.parse(fs.readFileSync(snapshot, 'utf8'));
      assert.equal(record.files['feature.txt'].hash, require('node:crypto').createHash('sha256').update('final job contents\n').digest('hex'));
    }
  }
});

test('explicit cache publication retains restore identity and snapshots before post cleanup', (t) => {
  const fixture = new Fixture(t);
  fixture.compilerCache();
  const env = { GITHUB_WORKSPACE: fixture.root, GITHUB_SHA: fixture.merge, GITHUB_OUTPUT: path.join(fixture.root, 'outputs'), VMUX_CACHE_PARTITION: 'test', VMUX_CACHE_TARGET: 'target', VMUX_CACHE_SAVE: 'true', RUNNER_OS: 'Linux', RUNNER_ARCH: 'X64' };
  Sources.configure(env);
  const snapshot = '.build-tools/source-cache-test.json';
  const inputs = JSON.parse(fs.readFileSync(path.join(fixture.root, `${snapshot}.inputs`), 'utf8'));
  const state = path.join(fixture.root, 'state');
  Sources.run({ GITHUB_WORKSPACE: fixture.root, GITHUB_STATE: state, INPUT_SNAPSHOT: snapshot, INPUT_TARGET: 'target', INPUT_RECEIPT: inputs.receipt, INPUT_SAVE: 'true' });
  const binary = fixture.compile();
  const modified = fs.statSync(binary, { bigint: true }).mtimeNs;
  env.RUSTFLAGS = '-C opt-level=1';
  env.GITHUB_SHA = 'b'.repeat(40);
  fs.writeFileSync(env.GITHUB_OUTPUT, '');
  Sources.checkpoint(env);
  const output = fs.readFileSync(env.GITHUB_OUTPUT, 'utf8');
  assert.ok(output.includes(inputs.key));
  assert.ok(output.includes(inputs.paths));
  assert.equal(fs.existsSync(path.join(fixture.root, inputs.receipt)), false);
  const sources = new Sources(fixture.root, snapshot, 'target');
  fs.utimesSync(path.join(fixture.root, 'src/main.rs'), new Date(), new Date());
  sources.restore();
  fixture.compile();
  assert.equal(execFileSync(binary, { encoding: 'utf8' }), 'original\n');
  assert.equal(fs.statSync(binary, { bigint: true }).mtimeNs, modified);
  assert.throws(() => Sources.checkpoint(env), /Missing.*receipt/);
});

test('identical sources restored from a compiler snapshot remain fresh to Cargo', (t) => {
  const fixture = new Fixture(t);
  const sources = fixture.compilerCache();
  const binary = fixture.compile();
  sources.save();
  const modified = fs.statSync(binary, { bigint: true }).mtimeNs;
  fs.utimesSync(path.join(fixture.root, 'src/main.rs'), new Date(), new Date());
  sources.restore();
  fixture.compile();
  assert.equal(fs.statSync(binary, { bigint: true }).mtimeNs, modified);
  assert.equal(execFileSync(binary, { encoding: 'utf8' }), 'original\n');
});

test('changed source older than cached compiler artifacts cannot reuse stale code', (t) => {
  for (const snapshot of ['valid', 'missing', 'corrupt', 'legacy']) {
    const fixture = new Fixture(t);
    const sources = fixture.compilerCache();
    fixture.compile();
    sources.save();
    if (snapshot === 'missing') fs.unlinkSync(sources.snapshot);
    if (snapshot === 'corrupt') fs.writeFileSync(sources.snapshot, '{bad snapshot');
    fs.writeFileSync(path.join(fixture.root, 'src/main.rs'), 'fn main() { println!("changed"); }\n');
    fs.utimesSync(path.join(fixture.root, 'src/main.rs'), 1, 1);
    if (snapshot === 'legacy') {
      const record = JSON.parse(fs.readFileSync(sources.snapshot, 'utf8'));
      record.version = 1;
      record.files['src/main.rs'].hash = require('node:crypto').createHash('sha256').update(fs.readFileSync(path.join(fixture.root, 'src/main.rs'))).digest('hex');
      record.files['src/main.rs'].mtime = '1000000000';
      fs.writeFileSync(sources.snapshot, JSON.stringify(record));
    }
    sources.restore();
    const binary = fixture.compile();
    assert.equal(execFileSync(binary, { encoding: 'utf8' }), 'changed\n', snapshot);
  }
});

test('restoring another target preserves fresh artifacts and rebuilds stale code', (t) => {
  for (const mode of ['instance', 'action']) {
    for (const changed of [false, true]) {
      const fixture = new Fixture(t);
      const first = fixture.compilerCache();
      const second = new Sources(fixture.root, '.alternate-cache.json', 'alternate-target');
      const file = path.join(fixture.root, 'src/main.rs');
      fs.writeFileSync(file, 'fn main() { println!("changed"); }\n');
      fs.utimesSync(file, 1, 1);
      fixture.compile('alternate-target');
      second.save();
      if (changed) fs.writeFileSync(file, 'fn main() { println!("original"); }\n');
      else fs.utimesSync(file, new Date(), new Date());
      const initial = fixture.compile();
      const modified = fs.statSync(initial, { bigint: true }).mtimeNs;
      first.save();
      fs.writeFileSync(file, 'fn main() { println!("changed"); }\n');
      let post;
      if (mode === 'action') post = fixture.restoreCache('.source-cache.json', 'target');
      else first.restore();
      second.restore();
      if (post) Sources.run(post);
      else first.save();
      first.restore();
      const binary = fixture.compile();
      assert.equal(execFileSync(binary, { encoding: 'utf8' }), 'changed\n', mode);
      if (!changed) assert.equal(fs.statSync(binary, { bigint: true }).mtimeNs, modified, mode);
    }
  }
});

test('isolated nightly timestamp restoration cannot rewind standard sources or reuse stale artifacts', (t) => {
  const fixture = new Fixture(t);
  const standard = fixture.compilerCache();
  const file = path.join(fixture.root, 'src/main.rs');
  const original = fs.readFileSync(file);
  const workspace = '.build-tools/compiler-checkout';
  const compiler = path.join(fixture.root, workspace);
  fs.mkdirSync(path.dirname(compiler), { recursive: true });
  execFileSync('git', ['clone', '--quiet', '--shared', fixture.root, compiler], { env: Preflight.gitEnvironment(), stdio: 'pipe' });
  fs.utimesSync(path.join(compiler, 'src/main.rs'), 1, 1);
  const nightly = new Sources(compiler, '.nightly-sources.json', 'nightly-target');
  nightly.save();
  fs.writeFileSync(file, 'fn main() { println!("stale cached output"); }\n');
  fixture.compile();
  standard.save();
  fs.writeFileSync(file, original);
  fs.utimesSync(file, 1, 1);
  standard.restore();
  const modified = fs.statSync(file, { bigint: true }).mtimeNs;
  Sources.run({
    GITHUB_WORKSPACE: fixture.root,
    GITHUB_STATE: path.join(fixture.root, 'nightly-state'),
    INPUT_WORKSPACE: workspace,
    INPUT_SNAPSHOT: '.nightly-sources.json',
    INPUT_TARGET: 'nightly-target',
    INPUT_SAVE: 'false',
  });
  assert.equal(fs.statSync(file, { bigint: true }).mtimeNs, modified);
  assert.equal(execFileSync(fixture.compile(), { encoding: 'utf8' }), 'original\n');
});

test('source cache workspaces reject escaping and symlinked checkout directories', (t) => {
  const fixture = new Fixture(t);
  const inputs = {
    GITHUB_WORKSPACE: fixture.root,
    GITHUB_STATE: path.join(fixture.root, 'state'),
    INPUT_SNAPSHOT: '.snapshot.json',
    INPUT_TARGET: 'target',
    INPUT_SAVE: 'false',
  };
  assert.throws(() => Sources.run({ ...inputs, INPUT_WORKSPACE: '..' }), /workspace escapes/);
  fs.mkdirSync(path.join(fixture.root, 'compiler'));
  fs.symlinkSync(path.join(fixture.root, 'compiler'), path.join(fixture.root, 'linked-compiler'));
  assert.throws(() => Sources.run({ ...inputs, INPUT_WORKSPACE: 'linked-compiler' }), /real directories/);
});

test('source timestamp restoration preserves files reached through symbolic links', (t) => {
  const fixture = new Fixture(t);
  const sources = fixture.compilerCache();
  const external = path.join(fixture.root, 'external.rs');
  fs.writeFileSync(external, 'external source');
  fs.symlinkSync(external, path.join(fixture.root, 'linked.rs'));
  fixture.git('add', 'linked.rs');
  sources.save();
  fs.writeFileSync(external, 'modified external source');
  fs.utimesSync(external, 1, 1);
  const before = fs.statSync(external, { bigint: true }).mtimeNs;
  sources.restore();
  assert.equal(fs.statSync(external, { bigint: true }).mtimeNs, before);
});

test('workspace caches isolate compiler flags and omit packaged runtime bundles', (t) => {
  const fixture = new Fixture(t);
  const sources = fixture.compilerCache();
  fixture.compile();
  const env = { GITHUB_SHA: fixture.head, VMUX_CACHE_PARTITION: 'test', RUNNER_OS: 'Linux', RUNNER_ARCH: 'X64' };
  const inputs = sources.inputs(env);
  const advanced = sources.inputs({ ...env, GITHUB_SHA: fixture.base });
  assert.equal(inputs.prefix, advanced.prefix);
  assert.notEqual(inputs.key, advanced.key);
  for (const flags of [{ RUSTFLAGS: '-C opt-level=1' }, { CARGO_ENCODED_RUSTFLAGS: '-C\x1fopt-level=1' }, { CARGO_PROFILE_RELEASE_LTO: 'off' }]) {
    assert.notEqual(sources.inputs({ ...env, ...flags }).prefix, inputs.prefix);
  }
  const globs = inputs.paths.split('\n');
  const includes = globs.filter((glob) => !glob.startsWith('!'));
  const excludes = globs.filter((glob) => glob.startsWith('!')).map((glob) => glob.slice(1));
  for (const artifact of ['target/debug/cache-fixture', 'target/debug/deps/cache_fixture-123456', 'target/aarch64-apple-ios/debug/deps/cache_fixture-123456']) {
    assert.equal(includes.some((glob) => path.matchesGlob(artifact, glob)), true);
    assert.equal(excludes.some((glob) => path.matchesGlob(artifact, glob)), false);
  }
  for (const bundle of ['target/debug/Vmux.app/Contents/MacOS/cache-fixture', 'target/release/Vmux.app.tar.gz', 'target/release/Vmux.dmg', 'target/debug/cache-fixture.app/Contents/MacOS/cache-fixture', 'target/debug/cache-fixture.tar.gz']) {
    assert.equal(includes.some((glob) => path.matchesGlob(bundle, glob)) && !excludes.some((glob) => path.matchesGlob(bundle, glob)), false, bundle);
  }
});

test('directory watchers stay fresh only while their tracked source set is identical', (t) => {
  const fixture = new Fixture(t);
  const sources = fixture.compilerCache('directory');
  const binary = fixture.compile();
  sources.save();
  const modified = fs.statSync(binary, { bigint: true }).mtimeNs;
  fs.utimesSync(path.join(fixture.root, 'src'), new Date(), new Date());
  sources.restore();
  fixture.compile();
  assert.equal(fs.statSync(binary, { bigint: true }).mtimeNs, modified);
  fs.unlinkSync(path.join(fixture.root, 'src/value.txt'));
  fs.utimesSync(path.join(fixture.root, 'src'), 1, 1);
  sources.restore();
  fixture.compile();
  assert.equal(execFileSync(binary, { encoding: 'utf8' }), '\n');
});

test('cached build metadata is refreshed for the current Git revision', (t) => {
  const fixture = new Fixture(t);
  const sources = fixture.compilerCache('git');
  const binary = fixture.compile();
  sources.save();
  fixture.git('commit', '--allow-empty', '-m', 'new identity');
  fs.utimesSync(path.join(fixture.root, '.git/HEAD'), 1, 1);
  fs.utimesSync(path.join(fixture.root, '.git/logs/HEAD'), 1, 1);
  sources.restore();
  fixture.compile();
  assert.equal(execFileSync(binary, { encoding: 'utf8' }).trim(), fixture.git('rev-parse', 'HEAD'));
});

test('replacing a Rust source with a tracked symlink cannot reuse the old executable', (t) => {
  const fixture = new Fixture(t);
  const sources = fixture.compilerCache();
  fs.writeFileSync(path.join(fixture.root, 'src/alternate.rs'), 'fn main() { println!("alternate"); }\n');
  fixture.git('add', 'src/alternate.rs');
  const binary = fixture.compile();
  sources.save();
  fs.unlinkSync(path.join(fixture.root, 'src/main.rs'));
  fs.symlinkSync('alternate.rs', path.join(fixture.root, 'src/main.rs'));
  sources.restore();
  fixture.compile();
  assert.equal(execFileSync(binary, { encoding: 'utf8' }), 'alternate\n');
});

test('cache locations reject symlink parents before any restoration', (t) => {
  for (const link of ['target', '.build-tools']) {
    const fixture = new Fixture(t);
    const external = fs.mkdtempSync(path.join(os.tmpdir(), 'vmux-external-cache-'));
    t.after(() => fs.rmSync(external, { recursive: true, force: true }));
    fs.symlinkSync(external, path.join(fixture.root, link));
    assert.throws(() => Sources.verify({ GITHUB_WORKSPACE: fixture.root }), /symbolic links/);
    assert.deepEqual(fs.readdirSync(external), []);
  }
});

class CheckFixture {
  constructor(t) {
    this.git = new Fixture(t);
    this.context = structuredClone(this.git.context);
    this.context.payload.pull_request.number = 1;
    const common = ['.github/**', 'scripts/**', 'Cargo.toml'];
    this.profiles = [
      { key: 'test-rust', name: 'Rust unit & integration tests', paths: [...common, 'crates/**'], scope: '-p example' },
      { key: 'website', name: 'Website Build', paths: [...common, 'website/**', 'docs/**'], scope: 'website-release' },
    ];
    this.advance({ 'crates/example/src/lib.rs': 'pub struct Example;\n', 'website/src/main.rs': 'web source\n', 'docs/guide.md': 'guide\n' });
    this.runs = [];
    this.jobs = {};
    this.artifacts = {};
    this.outputs = {};
    this.github = { rest: { actions: {
      listWorkflowRuns: async () => ({ data: { workflow_runs: this.runs } }),
      listJobsForWorkflowRun: async ({ run_id }) => ({ data: { jobs: this.jobs[run_id] } }),
      listWorkflowRunArtifacts: async ({ run_id }) => ({ data: { artifacts: this.artifacts[run_id] } }),
    } } };
    this.checks = new Checks({ github: this.github, context: this.context, root: this.git.root, core: { setOutput: (key, value) => { this.outputs[key] = value; }, info() {}, warning() {} } });
    this.prove(42);
  }

  advance(files) {
    this.git.git('checkout', 'feature');
    for (const [file, contents] of Object.entries(files)) {
      const destination = path.join(this.git.root, file);
      fs.mkdirSync(path.dirname(destination), { recursive: true });
      fs.writeFileSync(destination, contents);
    }
    this.git.head = this.git.commit('change inputs');
    this.identity = { base: this.git.base, tree: this.git.git('merge-tree', '--write-tree', this.git.base, this.git.head).split('\n')[0], branch: 'feature', release: false };
  }

  prove(id) {
    const claims = this.checks.inputs(this.identity, this.profiles, 'owner/repo', 1);
    this.runs.push({ id, event: 'pull_request', head_branch: 'feature', repository: { full_name: 'owner/repo' }, head_repository: { full_name: 'owner/repo' }, path: '.github/workflows/ci.yml', run_attempt: 1, html_url: `https://example.com/run/${id}` });
    this.jobs[id] = [];
    this.artifacts[id] = [];
    for (const profile of this.profiles) {
      this.jobs[id].push({ name: profile.name, status: 'completed', conclusion: 'success', run_attempt: 1 });
      this.artifacts[id].push({ name: `${claims[profile.key]}-1`, expired: false });
    }
  }

  async plan() {
    return this.checks.plan(this.identity, this.profiles);
  }
}

test('successful job evidence survives unrelated edits and only affected checks run again', async (t) => {
  const fixture = new CheckFixture(t);
  fixture.advance({ 'docs/guide.md': 'updated guide\n' });
  const docs = await fixture.plan();
  assert.equal(docs['test-rust'].reuse, '42');
  assert.equal(docs.website.reuse, '');
  fixture.advance({ 'docs/guide.md': 'guide\n', 'crates/example/src/lib.rs': 'pub struct Changed;\n' });
  const rust = await fixture.plan();
  assert.equal(rust['test-rust'].reuse, '');
  assert.equal(rust.website.reuse, '42');
});

test('successful configured workflow jobs remain reusable through their displayed names', async (t) => {
  const fixture = new CheckFixture(t);
  const workflow = new Workflow();
  fixture.profiles = await workflow.checkProfiles(fixture.identity, fixture.context);
  fixture.runs = [];
  fixture.prove(42);
  for (let index = 0; index < fixture.profiles.length; index++) {
    fixture.jobs[42][index].name = workflow.jobs[fixture.profiles[index].key].name;
  }
  const result = await fixture.plan();
  for (const profile of fixture.profiles) assert.equal(result[profile.key].reuse, '42', profile.key);
});

test('workflow fingerprints include ownership ledgers, native inputs and Cargo configuration', async (t) => {
  for (const file of ['ecs-policy.ron', 'cef/manifest.toml', 'packaging/Info.plist', '.cargo/config.toml', '.env']) {
    const fixture = new CheckFixture(t);
    fixture.profiles = await new Workflow().checkProfiles(fixture.identity, fixture.context);
    fixture.runs = [];
    fixture.prove(42);
    fixture.advance({ [file]: 'changed build or validation input\n' });
    assert.equal((await fixture.plan()).lint.reuse, '', file);
  }
});

test('dependency, CI, execution scope, base, deletion and executable-mode changes invalidate evidence', async (t) => {
  for (const change of ['dependency', 'workflow', 'scope', 'base', 'delete', 'mode']) {
    const fixture = new CheckFixture(t);
    if (change === 'dependency') fixture.advance({ 'Cargo.toml': '[workspace.package]\nversion = "2.0.0"\n' });
    if (change === 'workflow') fixture.advance({ '.github/workflows/ci.yml': 'changed validation flags\n' });
    if (change === 'scope') fixture.profiles[0].scope = '--workspace';
    if (change === 'base') fixture.identity.base = fixture.git.head;
    if (change === 'delete' || change === 'mode') {
      fixture.git.git('checkout', 'feature');
      const file = path.join(fixture.git.root, 'crates/example/src/lib.rs');
      if (change === 'delete') fs.unlinkSync(file);
      else fs.chmodSync(file, 0o755);
      fixture.advance({});
    }
    assert.equal((await fixture.plan())['test-rust'].reuse, '', change);
  }
});

test('a latest failed or incomplete job blocks older green evidence without discarding other passed jobs', async (t) => {
  for (const conclusion of ['failure', 'cancelled', 'timed_out', null]) {
    const fixture = new CheckFixture(t);
    fixture.prove(43);
    fixture.jobs[43][0].status = conclusion ? 'completed' : 'in_progress';
    fixture.jobs[43][0].conclusion = conclusion;
    const results = await fixture.plan();
    assert.equal(results['test-rust'].reuse, '', String(conclusion));
    assert.equal(results.website.reuse, '43');
  }
});

test('missing, expired, duplicate, and wrong-attempt evidence never grants reuse', async (t) => {
  for (const change of ['missing', 'expired', 'duplicate', 'attempt']) {
    const fixture = new CheckFixture(t);
    if (change === 'missing') fixture.artifacts[42] = [];
    if (change === 'expired') fixture.artifacts[42][0].expired = true;
    if (change === 'duplicate') fixture.jobs[42].push(fixture.jobs[42][0]);
    if (change === 'attempt') fixture.jobs[42][0].run_attempt = 2;
    assert.equal((await fixture.plan())['test-rust'].reuse, '', change);
  }
});

test('unread job pages cannot prove uniqueness of a successful check', async (t) => {
  const fixture = new CheckFixture(t);
  fixture.github.rest.actions.listJobsForWorkflowRun = async () => ({ data: { jobs: fixture.jobs[42], total_count: 101 } });
  assert.equal((await fixture.plan())['test-rust'].reuse, '');
});

test('reused or intentionally skipped runs can still refer to an earlier actual successful check', async (t) => {
  const fixture = new CheckFixture(t);
  fixture.prove(43);
  fixture.jobs[43][0].conclusion = 'skipped';
  fixture.artifacts[43] = [];
  const results = await fixture.plan();
  assert.equal(results['test-rust'].reuse, '42');
  assert.equal(results.website.reuse, '42');
});

test('foreign run provenance, fork PRs, releases and API failures require fresh checks', async (t) => {
  for (const change of ['repository', 'head_repository', 'branch', 'workflow', 'event', 'fork', 'release', 'api']) {
    const fixture = new CheckFixture(t);
    if (change === 'repository') fixture.runs[0].repository.full_name = 'other/repo';
    if (change === 'head_repository') fixture.runs[0].head_repository.full_name = 'fork/repo';
    if (change === 'branch') fixture.runs[0].head_branch = 'other';
    if (change === 'workflow') fixture.runs[0].path = '.github/workflows/other.yml';
    if (change === 'event') fixture.runs[0].event = 'push';
    if (change === 'fork') fixture.context.payload.pull_request.head.repo.full_name = 'fork/repo';
    if (change === 'release') fixture.identity.release = true;
    if (change === 'api') fixture.github.rest.actions.listWorkflowRuns = async () => { throw new Error('API unavailable'); };
    assert.equal((await fixture.plan())['test-rust'].reuse, '', change);
  }
});

test('symbolic-link inputs refuse reuse and claim writing preserves external targets', async (t) => {
  const fixture = new CheckFixture(t);
  fixture.git.git('checkout', 'feature');
  const file = path.join(fixture.git.root, 'crates/example/src/lib.rs');
  fs.unlinkSync(file);
  fs.symlinkSync('../../../feature.txt', file);
  fixture.advance({});
  assert.equal((await fixture.plan())['test-rust'].reuse, '');
  assert.equal(fixture.outputs.check_test_rust, '');
  const valid = new CheckFixture(t);
  const claim = (await valid.plan())['test-rust'].claim;
  Preflight.checkout({ VMUX_CI_HEAD: valid.git.head, VMUX_CI_BASE: valid.identity.base, VMUX_CI_TREE: valid.identity.tree }, valid.git.root);
  const env = { VMUX_CHECK_NAME: claim, VMUX_CHECK_TREE: valid.identity.tree, GITHUB_RUN_ID: '99', GITHUB_RUN_ATTEMPT: '1' };
  Checks.record(env, valid.git.root);
  const target = path.join(valid.git.root, 'external.json');
  fs.writeFileSync(target, 'preserve');
  const record = path.join(valid.git.root, 'ci-check-inputs.json');
  fs.unlinkSync(record);
  fs.symlinkSync(target, record);
  assert.throws(() => Checks.record(env, valid.git.root), /symbolic link/);
  assert.equal(fs.readFileSync(target, 'utf8'), 'preserve');
  assert.throws(() => Checks.record({ ...env, VMUX_CHECK_TREE: 'a'.repeat(40) }, valid.git.root), /differs/);
});

test('every validation artifact discovers its recorded claim with the default hidden-file exclusion', async (t) => {
  const fixture = new CheckFixture(t);
  const claim = (await fixture.plan())['test-rust'].claim;
  Preflight.checkout({ VMUX_CI_HEAD: fixture.git.head, VMUX_CI_BASE: fixture.identity.base, VMUX_CI_TREE: fixture.identity.tree }, fixture.git.root);
  Checks.record({ VMUX_CHECK_NAME: claim, VMUX_CHECK_TREE: fixture.identity.tree, GITHUB_RUN_ID: '99', GITHUB_RUN_ATTEMPT: '1' }, fixture.git.root);
  const workflow = new Workflow();
  for (const key of ['lint', 'test-rust', 'test-sdk', 'test-lean', 'test-headless', 'website']) {
    const upload = workflow.jobs[key].steps.find((step) => step.name === 'Publish validation inputs');
    const files = [];
    for (const file of fs.globSync(upload.with.path, { cwd: fixture.git.root })) {
      if (upload.with['include-hidden-files'] !== true && file.split(path.sep).some((part) => part.startsWith('.'))) continue;
      files.push(file);
    }
    assert.equal(files.length, 1, key);
    assert.equal(fs.statSync(path.join(fixture.git.root, files[0])).isFile(), true, key);
  }
});

test('Test accepts verified reused children but still rejects fresh failures and failed preparation', () => {
  const workflow = new Workflow();
  const env = { PLAN: 'success', CHANGES: 'success', CODE: 'true', RUST_REQUIRED: 'true', HEADLESS_REQUIRED: 'true', CONTRACTS: 'true', LEAN: 'true', RUST_TEST: 'skipped', SDK: 'skipped', HEADLESS: 'skipped', LEAN_TEST: 'skipped', REUSE_RUST_TEST: '42', REUSE_SDK: '42', REUSE_HEADLESS: '42', REUSE_LEAN_TEST: '42' };
  assert.equal(workflow.gate('test', env).status, 0);
  for (const key of ['REUSE_RUST_TEST', 'REUSE_SDK', 'REUSE_HEADLESS', 'REUSE_LEAN_TEST']) {
    assert.equal(workflow.gate('test', { ...env, [key]: '' }).status, 1, key);
  }
  assert.equal(workflow.gate('test', { ...env, CHANGES: 'failure' }).status, 1);
  assert.equal(workflow.lane('test-rust', 'pull_request', 'false', { code: 'true', rust: 'true', reuse_test_rust: '42' }), false);
});

test('SDK, headless and ordinary Rust requirements reject skips independently', () => {
  const workflow = new Workflow();
  const env = { PLAN: 'success', CHANGES: 'success', CODE: 'true', RUST_REQUIRED: 'false', HEADLESS_REQUIRED: 'false', CONTRACTS: 'false', LEAN: 'false', RUST_TEST: 'skipped', SDK: 'skipped', HEADLESS: 'skipped', LEAN_TEST: 'skipped' };
  assert.equal(workflow.gate('test', env).status, 0);
  for (const [required, result, job, selected] of [
    ['RUST_REQUIRED', 'RUST_TEST', 'test-rust', { rust: 'true' }],
    ['CONTRACTS', 'SDK', 'test-sdk', { contracts: 'true' }],
    ['HEADLESS_REQUIRED', 'HEADLESS', 'test-headless', { headless: 'true' }],
    ['LEAN', 'LEAN_TEST', 'test-lean', { lean: 'true' }],
  ]) {
    assert.equal(workflow.lane(job, 'pull_request', 'false', { code: 'true', ...selected }), true, job);
    assert.equal(workflow.gate('test', { ...env, [required]: 'true' }).status, 1, job);
    assert.equal(workflow.gate('test', { ...env, [required]: 'true', [result]: 'success' }).status, 0, job);
  }
});

test('shared build inputs select website and Rust validation while website manifests keep policy coverage', () => {
  const workflow = new Workflow();
  for (const file of ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'tool-versions.env', '.cargo/config.toml', '.github/workflows/ci.yml', 'scripts/ci-checks.js', 'website/Cargo.toml', 'website/Cargo.lock']) {
    const changes = workflow.changes('pull_request', 'false', 'base', 'true', workflow.paths([file]));
    assert.equal(changes.website, true, file);
    assert.equal(changes.code, true, file);
  }
  const policy = workflow.changes('pull_request', 'false', 'base', 'true', workflow.paths(['website/Cargo.toml']), 'false', 'false', { rust: 'false', sdk: 'false', headless: 'false', tooling: 'false' });
  for (const job of ['test-rust', 'test-sdk', 'test-headless', 'test-lean']) assert.equal(workflow.lane(job, 'pull_request', 'false', policy), false, job);
  const env = { PLAN: 'success', CHANGES: 'success', CODE: 'true', RUST_REQUIRED: 'false', HEADLESS_REQUIRED: 'false', CONTRACTS: 'false', LEAN: 'false' };
  assert.equal(workflow.gate('test', env).status, 0);
});

test('main and explicit runs keep full Rust validation even for documentation-only changes', () => {
  const workflow = new Workflow();
  for (const event of ['push', 'schedule', 'workflow_dispatch']) {
    const changes = workflow.changes(event, 'false', 'base', 'true', workflow.paths(['docs/ux/terminal.md']));
    assert.equal(changes.code, true, event);
    for (const job of ['test-rust', 'test-sdk', 'test-headless', 'test-lean']) assert.equal(workflow.lane(job, event, 'false', changes), true, `${event}: ${job}`);
  }
  const scope = workflow.jobs.changes.steps.find((step) => step.id === 'scope');
  for (const event of ['pull_request', 'push', 'schedule', 'workflow_dispatch']) {
    const comparison = Function('github', 'needs', `return (${scope.env.VMUX_CI_BASE.slice(3, -2)});`)({ event_name: event }, { plan: { outputs: { base: 'captured-base' } } });
    assert.equal(comparison, event === 'pull_request' ? 'captured-base' : '', event);
  }
});

test('empty package scopes cannot invoke Cargo default targets during deletion-only validation', () => {
  const workflow = new Workflow();
  for (const title of ['Check formatting', 'Clippy']) {
    assert.equal(workflow.step('lint', title, 'pull_request', 'false', { code: 'true', lint_args: '' }), false, title);
    assert.equal(workflow.step('lint', title, 'pull_request', 'false', { code: 'true', lint_args: '-p vmux_ecs' }), true, title);
  }
});

test('workflow reuse claims invalidate each variant only when its complete inputs change', async (t) => {
  for (const [file, expectedFresh] of [
    ['crates/unrelated/src/lib.rs', ['lint']],
    ['crates/util/vmux_mcp/src/lib.rs', ['lint', 'test-headless']],
    ['crates/util/vmux_app/src/lib.rs', ['lint', 'test-headless', 'test-sdk', 'test-lean']],
    ['crates/example/src/lib.rs', ['lint', 'test-rust']],
    ['website/src/main.rs', ['lint', 'website']],
    ['.github/workflows/ci.yml', ['lint', 'test-rust', 'test-sdk', 'test-lean', 'test-headless', 'website']],
  ]) {
    const fixture = new CheckFixture(t);
    fixture.advance({ 'crates/unrelated/src/lib.rs': 'unrelated', 'crates/util/vmux_mcp/src/lib.rs': 'mcp', 'crates/util/vmux_app/src/lib.rs': 'app' });
    fixture.profiles = await new Workflow().checkProfiles(fixture.identity, fixture.context, {
      'test-rust': ['crates/example/**'],
      'test-sdk': ['crates/util/vmux_app/**'],
      'test-headless': ['crates/util/vmux_app/**', 'crates/util/vmux_mcp/**'],
      'test-lean': ['crates/util/vmux_app/**'],
    });
    fixture.runs = [];
    fixture.prove(42);
    fixture.advance({ [file]: 'changed input' });
    const results = await fixture.plan();
    for (const profile of fixture.profiles) assert.equal(results[profile.key].reuse, expectedFresh.includes(profile.key) ? '' : '42', `${file}: ${profile.key}`);
  }
});
