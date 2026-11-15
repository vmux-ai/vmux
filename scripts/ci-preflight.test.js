const assert = require('node:assert/strict');
const { test } = require('node:test');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const Preflight = require('./ci-preflight.js');
const CliArtifact = require('./ci-cli.js');
const Sources = require('./ci-cache.js');

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

  changes(event, feature, base, native, filter = { macos: 'true', macos_package: 'true', macos_tooling: 'true', ios: 'true', code: 'true' }, lean = 'true', release = 'false') {
    const github = { event_name: event };
    const needs = { plan: { outputs: { feature, base, release } } };
    const steps = {
      filter: { outputs: filter },
      native: { outputs: { native } },
      scope: { outputs: { lean } },
    };
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

test('SDK and headless validation follow contract inputs while Git dependency checks stay outside PRs', () => {
  const workflow = new Workflow();
  for (const [file, contracts] of [
    ['crates/feature/vmux_terminal/assets/index.css', false],
    ['crates/feature/vmux_editor/src/ui.rs', false],
    ['docs/ux/terminal.md', false],
    ['crates/util/vmux_api/src/lib.rs', true],
    ['crates/util/vmux_app/src/lib.rs', true],
    ['crates/feature/vmux_acp/src/lib.rs', true],
    ['crates/feature/vmux_layout/src/feature.ron', true],
    ['crates/feature/vmux_extension/Cargo.toml', true],
    ['examples/framework-consumer/src/lib.rs', true],
    ['Cargo.lock', true],
  ]) {
    const changes = workflow.changes('pull_request', 'false', 'base', 'true', workflow.paths([file]), 'false');
    assert.equal(changes.contracts, contracts, file);
    assert.equal(workflow.lane('test-sdk', 'pull_request', 'false', changes), Boolean(changes.code && contracts), file);
    assert.equal(workflow.lane('test-headless', 'pull_request', 'false', changes), Boolean(changes.code && contracts), file);
    assert.equal(workflow.step('test-sdk', 'Test immutable Git dependencies', 'pull_request', 'false', changes), false);
  }
  assert.equal(workflow.step('test-sdk', 'Test immutable Git dependencies', 'push', 'false', {}), true);
});

test('feature variants remain parallel with ordinary tests and required when selected', () => {
  const workflow = new Workflow();
  for (const name of ['test-sdk', 'test-headless', 'test-lean']) {
    assert.equal(workflow.ready(name, { plan: 'success', changes: 'success', 'test-rust': 'in_progress' }), true);
  }
  assert.equal(workflow.lane('test-lean', 'pull_request', 'false', { code: 'true', contracts: 'false', lean: 'true' }), true);
  assert.equal(workflow.lane('test-lean', 'pull_request', 'false', { code: 'true', contracts: 'false', lean: 'false' }), false);
  const valid = { PLAN: 'success', CHANGES: 'success', CODE: 'true', CONTRACTS: 'false', LEAN: 'true', RUST_TEST: 'success', SDK: 'success', HEADLESS: 'success', LEAN_TEST: 'success' };
  assert.equal(workflow.gate('test', valid).status, 0);
  for (const result of ['failure', 'cancelled', 'skipped']) {
    assert.equal(workflow.gate('test', { ...valid, LEAN_TEST: result }).status, 1, result);
  }
});

test('required statuses fail on preparation errors and applicable child failures or unexpected skips', () => {
  const workflow = new Workflow();
  const valid = { PLAN: 'success', CHANGES: 'success', CODE: 'true', CONTRACTS: 'true', LEAN: 'false', RUST_TEST: 'success', SDK: 'success', HEADLESS: 'success', LEAN_TEST: 'success', DISTRIBUTION: 'true', MACOS: 'true', RESULT: 'success' };
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
