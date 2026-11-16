const { execFileSync } = require('node:child_process');
const Preflight = require('./ci-preflight.js');

class Workflow {
  static presentationOnly(root, base, tree) {
    if (!base || !tree) return false;
    try {
      Preflight.sha(base);
      Preflight.sha(tree);
      const options = { cwd: root, env: Preflight.gitEnvironment(), encoding: 'utf8', maxBuffer: 2 * 1024 * 1024, stdio: ['pipe', 'pipe', 'pipe'] };
      const changed = execFileSync('git', ['diff', '--name-only', '--no-renames', '-z', base, tree, '--'], options);
      if (changed !== '.github/workflows/ci.yml\0') return false;
      const file = '.github/workflows/ci.yml';
      const before = execFileSync('git', ['ls-tree', base, '--', file], options);
      const after = execFileSync('git', ['ls-tree', tree, '--', file], options);
      if (!before.startsWith('100644 blob ') || !after.startsWith('100644 blob ')) return false;
      const sources = [];
      for (const revision of [base, tree]) sources.push(execFileSync('git', ['show', `${revision}:${file}`], options));
      return Workflow.equivalent(sources[0], sources[1]);
    } catch {
      return false;
    }
  }

  static equivalent(before, after) {
    if (typeof before !== 'string' || typeof after !== 'string') return false;
    if (Buffer.byteLength(before) > 1024 * 1024 || Buffer.byteLength(after) > 1024 * 1024) return false;
    try {
      const result = execFileSync('ruby', ['-rpsych', '-rjson', '-e', Workflow.parser()], {
        input: JSON.stringify([before, after]), encoding: 'utf8', maxBuffer: 4 * 1024 * 1024, stdio: ['pipe', 'pipe', 'pipe'],
      });
      const normalized = JSON.parse(result);
      return JSON.stringify(normalized[0]) === JSON.stringify(normalized[1]);
    } catch {
      return false;
    }
  }

  static parser() {
    return `class WorkflowInput
  def self.normalize(node, path = [])
    case node
    when Psych::Nodes::Mapping
      pairs = []
      names = {}
      node.children.each_slice(2) do |key, value|
        raise 'Non-scalar mapping key' unless key.is_a?(Psych::Nodes::Scalar)
        raise 'Duplicate mapping key' if names[key.value]
        names[key.value] = true
        display = key.value == 'name' && path.length == 2 && path[0] == 'jobs' && ['plan', 'changes', 'ci'].include?(path[1])
        if display
          raise 'Non-literal display name' unless value.is_a?(Psych::Nodes::Scalar) && value.tag.nil? && !value.value.include?('$' + '{{')
          next
        end
        pairs << [normalize(key, path), normalize(value, path + [key.value])]
      end
      ['mapping', node.tag, pairs]
    when Psych::Nodes::Sequence
      children = []
      node.children.each_with_index do |child, index|
        children << normalize(child, path + [index])
      end
      ['sequence', node.tag, children]
    when Psych::Nodes::Scalar
      ['scalar', node.tag, node.value, node.plain, node.quoted, node.style]
    else
      raise 'Unsupported YAML node'
    end
  end

  def self.document(source)
    stream = Psych.parse_stream(source)
    raise 'One YAML document required' unless stream.children.length == 1
    root = stream.children[0].root
    raise 'Workflow mapping required' unless root.is_a?(Psych::Nodes::Mapping)
    normalize(root)
  end
end
documents = []
JSON.parse(STDIN.read).each do |source|
  documents << WorkflowInput.document(source)
end
puts JSON.generate(documents)
`;
  }
}

module.exports = Workflow;
