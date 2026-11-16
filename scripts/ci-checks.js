const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const Preflight = require('./ci-preflight.js');

class Checks {
  constructor({ github, context, core, root = process.cwd() } = {}) {
    this.github = github;
    this.context = context;
    this.core = core;
    this.root = root;
  }

  async plan(identity, profiles) {
    if (!Array.isArray(profiles) || profiles.length === 0 || profiles.length > 32) throw new Error('Invalid check profiles');
    const results = {};
    const names = new Set();
    for (const profile of profiles) {
      if (!/^[a-z][a-z0-9-]{0,47}$/.test(profile.key) || results[profile.key] || typeof profile.name !== 'string' || !profile.name || names.has(profile.name)) throw new Error('Invalid or duplicate check profile');
      names.add(profile.name);
      results[profile.key] = { claim: '', reuse: '' };
    }
    const pull = this.context.payload.pull_request;
    const repository = `${this.context.repo.owner}/${this.context.repo.repo}`;
    if (this.context.eventName === 'pull_request' && pull?.head.repo?.full_name === repository && !identity.release) {
      if (!Number.isSafeInteger(pull.number) || pull.number < 1) throw new Error('Invalid PR identity');
      const fingerprints = this.inputs(identity, profiles, repository, pull.number);
      for (const profile of profiles) results[profile.key].claim = profile.enabled === false ? '' : fingerprints[profile.key];
      try {
        await this.lookup(identity, profiles, results, repository);
      } catch {
        for (const profile of profiles) results[profile.key].reuse = '';
        this.core.warning('Previous check evidence is unavailable; running fresh validation');
      }
    }
    for (const profile of profiles) {
      const key = profile.key.replaceAll('-', '_');
      this.core.setOutput(`check_${key}`, results[profile.key].claim);
      this.core.setOutput(`reuse_${key}`, results[profile.key].reuse);
    }
    return results;
  }

  inputs(identity, profiles, repository, number) {
    Preflight.sha(identity.base);
    Preflight.sha(identity.tree);
    const output = execFileSync('git', ['ls-tree', '-r', '-z', identity.tree], {
      cwd: this.root, env: Preflight.gitEnvironment(), encoding: 'utf8', maxBuffer: 32 * 1024 * 1024,
    });
    const entries = [];
    for (const record of output.split('\0')) {
      if (!record) continue;
      const separator = record.indexOf('\t');
      if (separator < 0) throw new Error('Invalid Git tree record');
      const [mode, kind, sha] = record.slice(0, separator).split(' ');
      Preflight.sha(sha);
      entries.push({ mode, kind, sha, file: record.slice(separator + 1) });
    }
    const fingerprints = {};
    for (const profile of profiles) {
      if (!Array.isArray(profile.paths) || profile.paths.length === 0 || profile.paths.length > 128 || typeof profile.scope !== 'string' || profile.scope.length > 32768) throw new Error('Invalid check inputs');
      for (const pattern of profile.paths) if (typeof pattern !== 'string' || pattern.length > 512) throw new Error('Invalid check input pattern');
      const hash = crypto.createHash('sha256');
      hash.update(JSON.stringify({ version: 1, repository, number, base: identity.base, key: profile.key, name: profile.name, scope: profile.scope, paths: profile.paths }));
      let eligible = true;
      let count = 0;
      for (const entry of entries) {
        let selected = false;
        for (const pattern of profile.paths) {
          if (path.matchesGlob(entry.file, pattern)) { selected = true; break; }
        }
        if (!selected) continue;
        count++;
        if (entry.kind !== 'blob' || !['100644', '100755'].includes(entry.mode)) eligible = false;
        hash.update(`\0${entry.mode}\0${entry.file}\0${entry.sha}`);
      }
      fingerprints[profile.key] = eligible && count > 0 ? `ci-check-v1-${number}-${identity.base}-${profile.key}-${hash.digest('hex')}` : '';
    }
    return fingerprints;
  }

  async lookup(identity, profiles, results, repository) {
    const repo = this.context.repo;
    const { data } = await this.github.rest.actions.listWorkflowRuns({ ...repo, workflow_id: 'ci.yml', event: 'pull_request', branch: identity.branch, per_page: 20 });
    const settled = new Set();
    let active = 0;
    for (const profile of profiles) if (results[profile.key].claim) active++;
    const runs = data.workflow_runs.slice(0, 20).sort((left, right) => right.id - left.id);
    for (const run of runs) {
      if (!Number.isSafeInteger(run.id) || run.id < 1 || run.id === this.context.runId || run.event !== 'pull_request' || run.head_branch !== identity.branch || run.repository?.full_name !== repository || run.head_repository?.full_name !== repository || run.path !== '.github/workflows/ci.yml') continue;
      const listing = (await this.github.rest.actions.listJobsForWorkflowRun({ ...repo, run_id: run.id, filter: 'latest', per_page: 100 })).data;
      if (!Array.isArray(listing.jobs) || listing.jobs.length > 100 || listing.total_count > 100) throw new Error('Check job listing exceeds its verified bounds');
      const jobs = listing.jobs;
      let artifacts;
      for (const profile of profiles) {
        const result = results[profile.key];
        if (!result.claim || settled.has(profile.key)) continue;
        const matching = [];
        for (const job of jobs) if (job.name === profile.name) matching.push(job);
        if (matching.length === 0) continue;
        if (matching.length !== 1) { settled.add(profile.key); continue; }
        const job = matching[0];
        if (job.conclusion === 'skipped') continue;
        if (job.status !== 'completed' || job.conclusion !== 'success') { settled.add(profile.key); continue; }
        if (!artifacts) {
          const listing = (await this.github.rest.actions.listWorkflowRunArtifacts({ ...repo, run_id: run.id, per_page: 100 })).data;
          if (!Array.isArray(listing.artifacts) || listing.artifacts.length > 100 || listing.total_count > 100) throw new Error('Check artifact listing exceeds its verified bounds');
          artifacts = listing.artifacts;
        }
        const attempt = job.run_attempt ?? run.run_attempt;
        if (!Number.isSafeInteger(attempt) || attempt < 1) { settled.add(profile.key); continue; }
        const name = `${result.claim}-${attempt}`;
        let proven = false;
        for (const artifact of artifacts) if (!artifact.expired && artifact.name === name) proven = true;
        if (!proven) continue;
        result.reuse = String(run.id);
        settled.add(profile.key);
        this.core.info(`Reusing ${profile.name} from ${run.html_url}`);
      }
      if (settled.size === active) break;
    }
  }

  static matrix(profiles, results) {
    const include = [];
    for (const profile of profiles) {
      if (!profile.matrix || !profile.enabled) continue;
      if (!/^test-(rust|sdk|desktop|features|ecs)$/.test(profile.key)) throw new Error('Unknown test suite');
      for (const key of ['cache', 'dependencies']) {
        if (!/^[a-zA-Z0-9_-]+$/.test(profile.matrix[key] || '')) throw new Error('Invalid test cache partition');
      }
      if (!['true', 'false'].includes(profile.matrix.native)) throw new Error('Invalid test native configuration');
      const result = results[profile.key];
      if (!result || typeof result.claim !== 'string' || typeof result.reuse !== 'string' || (result.reuse && !/^[1-9]\d*$/.test(result.reuse))) throw new Error('Invalid test evidence');
      include.push({ key: profile.key, name: profile.name, cache: profile.matrix.cache, dependencies: profile.matrix.dependencies, native: profile.matrix.native, claim: result.claim, reuse: result.reuse });
    }
    return { include };
  }

  static gate(env = process.env) {
    const results = JSON.parse(env.VMUX_CI_RESULTS);
    Checks.requireResult(results, 'plan', true);
    Checks.requireResult(results, 'changes', true);
    const outputs = results.changes.outputs;
    for (const key of ['code', 'rust', 'contracts', 'desktop', 'headless', 'legacy_desktop', 'website', 'macos', 'ios', 'macos_tooling']) {
      if (!['true', 'false'].includes(outputs[key])) throw new Error(`Invalid CI selection: ${key}`);
    }
    const matrix = JSON.parse(outputs.tests);
    if (!Array.isArray(matrix.include) || matrix.include.length > 5 || String(matrix.include.length) !== outputs.test_count) throw new Error('Invalid test matrix');
    const code = outputs.code === 'true';
    const selected = new Set();
    for (const entry of matrix.include) {
      if (selected.has(entry.key)) throw new Error('Duplicate test suite');
      selected.add(entry.key);
    }
    for (const [key, required] of [
      ['test-rust', code && outputs.rust === 'true'],
      ['test-sdk', code && outputs.contracts === 'true'],
      ['test-desktop', code && outputs.desktop === 'true'],
      ['test-features', code && (outputs.headless === 'true' || outputs.legacy_desktop === 'true')],
      ['test-ecs', code],
    ]) {
      if (selected.has(key) !== required) throw new Error(`Test selection differs from required scope: ${key}`);
      selected.delete(key);
    }
    if (selected.size) throw new Error('Unknown test selection');
    Checks.requireResult(results, 'tests', matrix.include.length > 0);
    Checks.requireResult(results, 'lint', true);
    Checks.requireResult(results, 'website', outputs.website === 'true', Boolean(outputs.reuse_website));
    if (!['true', 'false'].includes(env.VMUX_CI_DISTRIBUTION)) throw new Error('Invalid distribution policy');
    const distribution = env.VMUX_CI_DISTRIBUTION === 'true';
    Checks.requireResult(results, 'dx-macos', distribution && (outputs.macos === 'true' || outputs.ios === 'true' || outputs.macos_tooling === 'true'));
    Checks.requireResult(results, 'build-mac', distribution && outputs.macos === 'true');
    Checks.requireResult(results, 'ios-build', outputs.ios === 'true');
  }

  static requireResult(results, name, required, reused = false) {
    const expected = required && !reused ? 'success' : 'skipped';
    if (results[name]?.result !== expected) throw new Error(`CI ${name}: expected ${expected}, received ${results[name]?.result || 'missing'}`);
  }

  static record(env = process.env, root = process.cwd()) {
    if (!/^ci-check-v1-\d+-[a-f0-9]{40}-[a-z][a-z0-9-]*-[a-f0-9]{64}$/.test(env.VMUX_CHECK_NAME || '') || !/^\d+$/.test(env.GITHUB_RUN_ID || '') || !/^[1-9]\d*$/.test(env.GITHUB_RUN_ATTEMPT || '')) throw new Error('Invalid check claim identity');
    Preflight.sha(env.VMUX_CHECK_TREE);
    const tree = execFileSync('git', ['rev-parse', 'HEAD^{tree}'], { cwd: root, env: Preflight.gitEnvironment(), encoding: 'utf8' }).trim();
    if (tree !== env.VMUX_CHECK_TREE) throw new Error('Check checkout differs from its planned inputs');
    const file = path.join(root, 'ci-check-inputs.json');
    if (fs.lstatSync(file, { throwIfNoEntry: false })?.isSymbolicLink()) throw new Error('Check claim path must not be a symbolic link');
    const descriptor = fs.openSync(file, fs.constants.O_CREAT | fs.constants.O_WRONLY | fs.constants.O_TRUNC | fs.constants.O_NOFOLLOW, 0o600);
    try {
      fs.writeFileSync(descriptor, `${JSON.stringify({ version: 1, name: env.VMUX_CHECK_NAME, run: env.GITHUB_RUN_ID, attempt: env.GITHUB_RUN_ATTEMPT, tree })}\n`);
    } finally {
      fs.closeSync(descriptor);
    }
  }
}

module.exports = Checks;

if (require.main === module) {
  if (process.argv[2] === 'record') Checks.record();
  else if (process.argv[2] === 'gate') Checks.gate();
  else throw new Error('Expected record or gate');
}
