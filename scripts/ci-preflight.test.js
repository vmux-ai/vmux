const assert = require('node:assert/strict');
const { test } = require('node:test');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const Preflight = require('./ci-preflight.js');

class Fixture {
  constructor(t) {
    this.root = fs.mkdtempSync(path.join(os.tmpdir(), 'vmux-preflight-'));
    t.after(() => fs.rmSync(this.root, { recursive: true, force: true }));
    this.git('init', '-b', 'main');
    this.git('config', 'user.name', 'Fixture');
    this.git('config', 'user.email', 'fixture@example.com');
    this.git('config', 'commit.gpgsign', 'false');
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
        repos: { getBranch: async ({ branch }) => { this.baseBranch = branch; return { data: { commit: { sha: this.base } } }; } },
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
    return execFileSync('git', args, { cwd: this.root, encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }).trim();
  }

  commit(message) {
    this.git('add', '.');
    this.git('commit', '-m', message);
    return this.git('rev-parse', 'HEAD');
  }
}

test('completed branch checks are reused only for the exact base/head merge', async (t) => {
  const fixture = new Fixture(t);
  assert.equal((await fixture.preflight.plan()).reuse, '42');
  assert.equal(fixture.sleeps, 0);
  assert.equal(fixture.outputs.tree, fixture.tree);
});

test('running branch CI is joined before accepting its completed evidence', async (t) => {
  const fixture = new Fixture(t);
  fixture.run.status = 'in_progress';
  fixture.run.conclusion = null;
  assert.equal((await fixture.preflight.plan()).reuse, '42');
  assert.equal(fixture.polls, 1);
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
  const plan = await release.preflight.plan();
  assert.equal(plan.release, true);
  assert.equal(plan.reuse, '');
  assert.equal(release.outputs.receipt, '');
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
  fixture.context.sha = fixture.head;
  await assert.rejects(fixture.preflight.plan(), /does not match/);
});

test('prepared checkouts compile the merged code from both branches', (t) => {
  const fixture = new Fixture(t);
  fixture.git('checkout', '--detach', fixture.head);
  assert.equal(fs.existsSync(path.join(fixture.root, 'base.txt')), false);
  const env = { VMUX_CI_HEAD: fixture.head, VMUX_CI_BASE: fixture.base, VMUX_CI_TREE: fixture.tree };
  Preflight.checkout(env, fixture.root);
  assert.equal(fixture.git('rev-parse', 'HEAD^{tree}'), fixture.tree);
  assert.equal(fs.readFileSync(path.join(fixture.root, 'base.txt'), 'utf8'), 'base\n');
  assert.equal(fs.readFileSync(path.join(fixture.root, 'feature.txt'), 'utf8'), 'feature\n');
  assert.deepEqual(fixture.git('diff', '--name-only', fixture.base, 'HEAD').split('\n'), ['feature.txt']);
  assert.equal(fixture.git('status', '--porcelain'), '');
  assert.throws(() => Preflight.checkout({ ...env, VMUX_CI_TREE: 'a'.repeat(40) }, fixture.root), /changed/);
  assert.throws(() => Preflight.checkout({ ...env, VMUX_CI_BASE: '--help' }, fixture.root), /Invalid/);
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
    plan: { result: 'success', outputs: { ...fixture.identity, feature: 'true', release: 'false', reuse: '', receipt: Preflight.artifact(fixture.identity) } },
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
    this.jobs = JSON.parse(execFileSync('ruby', ['-rjson', '-ryaml', '-e', 'puts JSON.generate(YAML.load_file(ARGV[0])["jobs"])', file], { encoding: 'utf8' }));
  }

  gate(name, env) {
    return require('node:child_process').spawnSync('bash', ['-c', this.jobs[name].steps[0].run], { env: { ...process.env, ...env }, encoding: 'utf8' });
  }

  selected(name, event, feature, macos, packaging, reuse = '') {
    const github = { event_name: event };
    const needs = { plan: { outputs: { feature, reuse } }, changes: { outputs: { macos, macos_package: packaging } } };
    return Function('github', 'needs', `return (${this.jobs[name].if});`)(github, needs);
  }

  name(name, event, feature) {
    const expression = this.jobs[name].name;
    if (!expression.startsWith('${{')) return expression;
    return Function('github', 'needs', `return (${expression.slice(3, -2)});`)({ event_name: event }, { plan: { outputs: { feature } } });
  }
}

test('branch evidence cannot satisfy required PR checks before merge-tree verification', () => {
  const workflow = new Workflow();
  const names = ['lint', 'test', 'build-macos', 'build-ios'];
  const required = ['Lint', 'Test', 'Build macOS', 'Build iOS'];
  for (const [event, feature] of [['pull_request', 'false'], ['push', 'false']]) {
    assert.deepEqual(names.map((name) => workflow.name(name, event, feature)), required);
  }
  for (const name of names) assert.equal(required.includes(workflow.name(name, 'push', 'true')), false);
});

test('required workflow gates reject failed planning and unexpected child skips', () => {
  const workflow = new Workflow();
  const valid = { PLAN: 'success', PREFLIGHT: '', CHANGES: 'success', CODE: 'true', FORMAT: 'success', RESULT: 'success', DEPS: 'success', RUST_TEST: 'success', POLICY: 'success', SDK_PATH: 'success', SDK_GIT: 'success', HEADLESS: 'success', MACOS: 'true', PACKAGE: 'true', CHECK: 'skipped', IOS: 'true' };
  for (const name of ['lint', 'test', 'build-macos', 'build-ios']) {
    assert.equal(workflow.gate(name, valid).status, 0, name);
    assert.equal(workflow.gate(name, { ...valid, PLAN: 'failure', PREFLIGHT: '42' }).status, 1, name);
    assert.equal(workflow.gate(name, { ...valid, CHANGES: 'failure' }).status, 1, name);
    assert.equal(workflow.gate(name, { ...valid, PREFLIGHT: '42', CHANGES: 'skipped', RESULT: 'skipped', POLICY: 'skipped' }).status, 0, name);
  }
  for (const result of ['failure', 'cancelled', 'skipped']) {
    for (const lane of ['RUST_TEST', 'POLICY', 'SDK_PATH', 'SDK_GIT', 'HEADLESS']) {
      assert.equal(workflow.gate('test', { ...valid, [lane]: result }).status, 1, lane);
    }
    assert.equal(workflow.gate('lint', { ...valid, RESULT: result }).status, 1);
    assert.equal(workflow.gate('build-macos', { ...valid, RESULT: result }).status, 1);
    assert.equal(workflow.gate('build-ios', { ...valid, RESULT: result }).status, 1);
  }
});

test('feature pushes and PRs use type checks unless packaging inputs changed', () => {
  const workflow = new Workflow();
  for (const [event, feature, macos, packaging, check, build] of [
    ['push', 'true', 'true', 'false', true, false],
    ['pull_request', 'false', 'true', 'false', true, false],
    ['push', 'true', 'true', 'true', false, true],
    ['pull_request', 'false', 'true', 'true', false, true],
    ['push', 'false', 'true', 'false', false, true],
    ['workflow_dispatch', 'false', 'true', 'false', false, true],
    ['schedule', 'false', 'false', 'false', false, false],
    ['push', 'true', 'false', 'false', false, false],
  ]) {
    assert.equal(workflow.selected('check-mac', event, feature, macos, packaging), check);
    assert.equal(workflow.selected('build-mac', event, feature, macos, packaging), build);
  }
  assert.equal(workflow.selected('changes', 'pull_request', 'false', '', '', '42'), false);
  assert.equal(workflow.selected('cef-version', 'pull_request', 'false', '', '', '42'), false);
});

class Package {
  constructor(t) {
    this.root = fs.mkdtempSync(path.join(os.tmpdir(), 'vmux-ci-package-'));
    t.after(() => fs.rmSync(this.root, { recursive: true, force: true }));
    for (const directory of ['scripts', 'bin', 'crates/app/vmux_desktop', 'packaging/macos']) fs.mkdirSync(path.join(this.root, directory), { recursive: true });
    for (const name of ['package.sh', 'cargo-target-paths.sh']) fs.copyFileSync(path.join(__dirname, name), path.join(this.root, 'scripts', name));
    fs.writeFileSync(path.join(this.root, 'crates/app/vmux_desktop/Cargo.toml'), 'fixture manifest\n');
    fs.writeFileSync(path.join(this.root, 'packaging/macos/Info.plist'), 'fixture plist\n');
    this.script('bin/sed', '#!/bin/sh\nexit 0\n');
    this.script('scripts/cargo-with-cef-cache.sh', '#!/bin/sh\nprintf "%s\\n" "$*" >> "$TEST_LOG"\nmkdir -p "$VMUX_APP_BUNDLE"\n');
    for (const name of ['inject-cef.sh', 'embed-launch-agent-plist.sh', 'sign-and-notarize.sh']) {
      this.script(`scripts/${name}`, `#!/bin/sh\nprintf '${name}\\n' >> "$TEST_LOG"\n`);
    }
  }

  script(name, content) {
    fs.writeFileSync(path.join(this.root, name), content, { mode: 0o755 });
  }

  run(env) {
    this.log = path.join(this.root, 'package.log');
    fs.rmSync(this.log, { force: true });
    return require('node:child_process').spawnSync('bash', [path.join(this.root, 'scripts/package.sh'), 'release'], {
      env: { ...process.env, PATH: `${this.root}/bin:${process.env.PATH}`, VMUX_CI_FAST_PACKAGE: '1', IS_RELEASE_PR: '0', TEST_LOG: this.log, CARGO_TARGET_DIR: `${this.root}/output with spaces`, ...env },
      encoding: 'utf8',
    });
  }
}

test('fast feature packaging keeps app injection and signing and rejects other pushes', (t) => {
  const fixture = new Package(t);
  for (const env of [
    { GITHUB_EVENT_NAME: 'pull_request' },
    { GITHUB_EVENT_NAME: 'push', GITHUB_REF: 'refs/heads/feature', VMUX_CI_PREFLIGHT: 'true' },
  ]) {
    const result = fixture.run(env);
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(fs.readFileSync(fixture.log, 'utf8').trim().split('\n'), ['packager --release --formats app', 'inject-cef.sh', 'embed-launch-agent-plist.sh', 'sign-and-notarize.sh']);
  }
  for (const env of [
    { GITHUB_EVENT_NAME: 'push', GITHUB_REF: 'refs/heads/feature', VMUX_CI_PREFLIGHT: '' },
    { GITHUB_EVENT_NAME: 'push', GITHUB_REF: 'refs/heads/main', VMUX_CI_PREFLIGHT: 'true' },
    { GITHUB_EVENT_NAME: 'push', GITHUB_REF: 'refs/heads/master', VMUX_CI_PREFLIGHT: 'true' },
    { GITHUB_EVENT_NAME: 'push', GITHUB_REF: 'refs/heads/release/1.0', VMUX_CI_PREFLIGHT: 'true' },
    { GITHUB_EVENT_NAME: 'push', GITHUB_REF: 'refs/tags/v1.0', VMUX_CI_PREFLIGHT: 'true' },
    { GITHUB_EVENT_NAME: 'workflow_dispatch' },
    { GITHUB_EVENT_NAME: 'pull_request', IS_RELEASE_PR: '1' },
  ]) {
    const result = fixture.run(env);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Fast package mode requires/);
    assert.equal(fs.existsSync(fixture.log), false);
  }
});
