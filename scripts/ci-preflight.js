const { execFileSync } = require('node:child_process');
const fs = require('node:fs');

class Preflight {
  constructor({ github, context, core, root = process.cwd(), pause, now }) {
    this.github = github;
    this.context = context;
    this.core = core;
    this.root = root;
    this.environment = Preflight.gitEnvironment();
    this.pause = pause || ((milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds)));
    this.now = now || Date.now;
  }

  async plan() {
    const { payload, eventName, repo, sha, ref } = this.context;
    const pull = payload.pull_request;
    const head = pull ? pull.head.sha : sha;
    let base = pull ? pull.base.sha : '';
    const branch = pull ? pull.head.ref : ref.replace(/^refs\/heads\//, '');
    const feature = eventName === 'push' && ![payload.repository.default_branch, 'main', 'master'].includes(branch) && !branch.startsWith('release/');
    let baseBranch = pull ? pull.base.ref : payload.repository.default_branch;
    if (feature) {
      const { data } = await this.github.rest.pulls.list({ ...repo, state: 'open', head: `${repo.owner}:${branch}`, per_page: 100 });
      const bases = new Set(data.map((item) => item.base.ref));
      if (bases.size > 1) throw new Error('Branch has pull requests with different bases; use PR CI');
      if (data.length) baseBranch = data[0].base.ref;
      base = (await this.github.rest.repos.getBranch({ ...repo, branch: baseBranch })).data.commit.sha;
    }
    let tree = '';
    let release = false;
    if (base) {
      this.fetch(base);
      this.fetch(head);
      tree = this.git('merge-tree', '--write-tree', base, head).split('\n')[0];
      Preflight.sha(tree);
      release = this.version(base) !== this.version(head);
      if (pull && this.git('rev-parse', `${sha}^{tree}`) !== tree) {
        throw new Error('PR checkout does not match its base/head merge tree');
      }
    }
    const identity = { head, base, tree, branch, repository: `${repo.owner}/${repo.repo}` };
    let reuse = '';
    if (pull && !release && pull.head.repo?.full_name === identity.repository) {
      reuse = await this.reuse(identity);
    }
    for (const [name, value] of Object.entries({ head, base, tree, feature, release, reuse, receipt: base && !release ? Preflight.artifact(identity) : '' })) {
      this.core.setOutput(name, String(value));
    }
    if (feature && !release) fs.writeFileSync(`${this.root}/ci-preflight-inputs.json`, `${JSON.stringify(identity)}\n`);
    return { ...identity, feature, release, reuse };
  }

  async reuse(identity) {
    const { repo, runId } = this.context;
    let candidates = [];
    for (let attempt = 0; attempt < 5; attempt++) {
      const { data } = await this.github.rest.actions.listWorkflowRuns({ ...repo, workflow_id: 'ci.yml', event: 'push', head_sha: identity.head, per_page: 30 });
      candidates = data.workflow_runs.filter((run) => Preflight.candidate(run, identity) && run.id !== runId);
      if (candidates.length) break;
      if (attempt < 4) await this.pause(5000);
    }
    for (let run of candidates) {
      if (run.status !== 'completed') {
        if (!(await this.inputs(run, identity))) continue;
        this.core.info(`Waiting for branch CI: ${run.html_url}`);
        const deadline = this.now() + 85 * 60 * 1000;
        while (run.status !== 'completed') {
          if (this.now() >= deadline) throw new Error(`Branch CI did not finish within 85 minutes: ${run.html_url}`);
          await this.pause(15000);
          run = (await this.github.rest.actions.getWorkflowRun({ ...repo, run_id: run.id })).data;
          if (!Preflight.candidate(run, identity)) throw new Error('Branch CI provenance changed while waiting');
        }
        if (run.conclusion !== 'success') throw new Error(`Branch CI ${run.conclusion}: ${run.html_url}`);
      }
      if (run.conclusion !== 'success') continue;
      const artifacts = await this.github.paginate(this.github.rest.actions.listWorkflowRunArtifacts, { ...repo, run_id: run.id, per_page: 100 });
      if (!artifacts.some((artifact) => !artifact.expired && artifact.name === Preflight.artifact(identity))) continue;
      const jobs = await this.github.paginate(this.github.rest.actions.listJobsForWorkflowRun, { ...repo, run_id: run.id, filter: 'latest', per_page: 100 });
      if (!Preflight.gates(jobs)) continue;
      this.core.info(`Reusing verified branch CI: ${run.html_url}`);
      return String(run.id);
    }
    this.core.info('No matching successful preflight; running normal PR checks');
    return '';
  }

  async inputs(run, identity) {
    for (let attempt = 0; attempt < 5; attempt++) {
      const artifacts = await this.github.paginate(this.github.rest.actions.listWorkflowRunArtifacts, { ...this.context.repo, run_id: run.id, per_page: 100 });
      for (const artifact of artifacts) {
        if (!artifact.name.startsWith('inputs-ci-preflight-v1-')) continue;
        return !artifact.expired && artifact.name === `inputs-${Preflight.artifact(identity)}`;
      }
      if (attempt < 4) await this.pause(5000);
    }
    return false;
  }

  fetch(sha) {
    Preflight.sha(sha);
    try {
      this.git('cat-file', '-e', `${sha}^{commit}`);
    } catch {
      this.git('fetch', '--no-tags', 'origin', sha);
    }
  }

  version(sha) {
    const version = this.git('show', `${sha}:Cargo.toml`).match(/^version\s*=\s*"([^"]+)"/m);
    if (!version) throw new Error('Could not resolve workspace version');
    return version[1];
  }

  git(...args) {
    return execFileSync('git', args, { cwd: this.root, env: this.environment, encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }).trim();
  }

  static gitEnvironment() {
    const environment = { ...process.env };
    for (const name of ['GIT_ALTERNATE_OBJECT_DIRECTORIES', 'GIT_CONFIG', 'GIT_CONFIG_PARAMETERS', 'GIT_CONFIG_COUNT', 'GIT_OBJECT_DIRECTORY', 'GIT_DIR', 'GIT_WORK_TREE', 'GIT_IMPLICIT_WORK_TREE', 'GIT_GRAFT_FILE', 'GIT_INDEX_FILE', 'GIT_NO_REPLACE_OBJECTS', 'GIT_REPLACE_REF_BASE', 'GIT_PREFIX', 'GIT_SHALLOW_FILE', 'GIT_COMMON_DIR']) {
      delete environment[name];
    }
    return environment;
  }

  static candidate(run, identity) {
    return run.event === 'push' && run.head_sha === identity.head && run.head_branch === identity.branch
      && run.repository?.full_name === identity.repository && run.head_repository?.full_name === identity.repository
      && run.path === '.github/workflows/ci.yml';
  }

  static artifact({ head, base, tree }) {
    for (const sha of [head, base, tree]) Preflight.sha(sha);
    return `ci-preflight-v1-${head}-${base}-${tree}`;
  }

  static gates(jobs) {
    for (const name of ['Preflight Lint', 'Preflight Test', 'Preflight Build macOS', 'Preflight Build iOS', 'Record preflight']) {
      const matching = jobs.filter((job) => job.name === name);
      if (matching.length !== 1 || matching[0].conclusion !== 'success') return false;
    }
    return true;
  }

  static sha(value) {
    if (!/^[a-f0-9]{40}$/.test(value)) throw new Error('Invalid preflight Git object ID');
  }

  static checkout(env = process.env, root = process.cwd()) {
    if (!env.VMUX_CI_BASE) return;
    for (const sha of [env.VMUX_CI_BASE, env.VMUX_CI_HEAD, env.VMUX_CI_TREE]) Preflight.sha(sha);
    const git = new Preflight({ root });
    git.fetch(env.VMUX_CI_BASE);
    git.fetch(env.VMUX_CI_HEAD);
    const tree = git.git('merge-tree', '--write-tree', env.VMUX_CI_BASE, env.VMUX_CI_HEAD).split('\n')[0];
    if (tree !== env.VMUX_CI_TREE) throw new Error('CI merge tree changed after planning');
    const date = git.git('show', '-s', '--format=%cI', env.VMUX_CI_HEAD);
    const commit = execFileSync('git', ['commit-tree', tree, '-p', env.VMUX_CI_BASE, '-p', env.VMUX_CI_HEAD], {
      cwd: root, encoding: 'utf8', input: 'CI merge validation\n',
      env: { ...git.environment, GIT_AUTHOR_NAME: 'CI', GIT_COMMITTER_NAME: 'CI', GIT_AUTHOR_EMAIL: 'ci@vmux.ai', GIT_COMMITTER_EMAIL: 'ci@vmux.ai', GIT_AUTHOR_DATE: date, GIT_COMMITTER_DATE: date },
    }).trim();
    git.git('checkout', '--detach', commit);
  }

  static record(env = process.env, root = process.cwd()) {
    const results = JSON.parse(env.VMUX_CI_RESULTS);
    for (const name of ['plan', 'changes', 'lint', 'test', 'build-macos', 'build-ios']) {
      if (results[name]?.result !== 'success') throw new Error(`Cannot record preflight: ${name} did not succeed`);
    }
    const website = results.changes.outputs.website === 'true';
    if (results.website?.result !== (website ? 'success' : 'skipped')) throw new Error('Cannot record preflight: unexpected website result');
    const plan = results.plan.outputs;
    if (plan.feature !== 'true' || plan.release !== 'false' || plan.reuse) throw new Error('Only original non-release branch CI can record preflight');
    const identity = { head: plan.head, base: plan.base, tree: plan.tree };
    const name = Preflight.artifact(identity);
    if (plan.receipt !== name) throw new Error('Preflight artifact identity does not match the plan');
    fs.writeFileSync(`${root}/ci-preflight.json`, `${JSON.stringify({ version: 1, ...identity, run: env.GITHUB_RUN_ID })}\n`);
  }
}

module.exports = Preflight;

if (require.main === module) {
  if (process.argv[2] === 'checkout') Preflight.checkout();
  else if (process.argv[2] === 'record') Preflight.record();
  else throw new Error('Expected checkout or record');
}
