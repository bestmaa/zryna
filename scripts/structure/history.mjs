import { POLICY_PATH, fail, physicalLines, validatePolicy } from './policy.mjs';
import { blobs, commit, git, legacySource, source, tree } from './repository.mjs';

// Once adopted, the policy in the independently selected trusted base authenticates
// its immutable inventory. A squash need not retain the original bootstrap objects.
export function history(root, comparison, head, policy, trustedPolicy) {
  if (trustedPolicy && JSON.stringify(policy.baseline) !== JSON.stringify(trustedPolicy.baseline)) {
    fail('baseline differs from trusted base policy');
  }
  let origin = policy.anchor;
  let ratchetBase = comparison;
  if (trustedPolicy) {
    if (git(root, ['rev-parse', '--is-shallow-repository']).trim() !== 'false') {
      fail('Git comparison authority unavailable: supply complete policy adoption history');
    }
    const additions = git(root, ['log', '--format=%H', '--diff-filter=A', comparison, '--', POLICY_PATH])
      .trim().split('\n').filter(Boolean);
    if (additions.length !== 1) fail('policy adoption history is absent or ambiguous');
    [origin] = additions;
    const adoptedEntry = tree(root, origin).get(POLICY_PATH);
    if (adoptedEntry?.mode !== '100644') fail('adopted policy is not a regular file');
    const adopted = validatePolicy(JSON.parse(blobs(root, [adoptedEntry]).get(adoptedEntry.hash)), '0000-00-00');
    if (adopted.anchor !== policy.anchor || JSON.stringify(adopted.baseline) !== JSON.stringify(policy.baseline)) {
      fail('anchor or baseline differs from original trusted policy adoption');
    }
  } else {
    commit(root, origin);
    git(root, ['merge-base', '--is-ancestor', origin, head]);
    const common = git(root, ['merge-base', origin, comparison]).trim();
    if (common !== origin) {
      if (common !== comparison) fail('trusted base and initial anchor are incomparable');
      ratchetBase = origin;
    }
  }
  const original = tree(root, origin);
  const before = tree(root, ratchetBase);
  const historical = blobs(root, [...original.entries(), ...before.entries()]
    .filter(([path, entry]) => source(path) && ['100644', '100755'].includes(entry.mode))
    .map(([, entry]) => entry));
  if (!trustedPolicy) {
    const excluded = new Set(policy.classifications.map(entry => entry.path));
    const inventory = [...original.entries()].filter(([path, entry]) => legacySource(path)
      && historical.has(entry.hash) && physicalLines(historical.get(entry.hash)) > 500)
      .map(([path, entry]) => ({ path, lines: physicalLines(historical.get(entry.hash)), production: !excluded.has(path) }));
    if (JSON.stringify(policy.baseline) !== JSON.stringify(inventory)) {
      fail('baseline differs from exact anchored source inventory; do not raise or regenerate allowances');
    }
  }
  // Adoption can only lower frozen ceilings, never regenerate them from larger files.
  const ceilings = new Map(policy.baseline.filter(entry => entry.production).map(entry => {
    const adopted = original.get(entry.path);
    const size = adopted && historical.has(adopted.hash) ? physicalLines(historical.get(adopted.hash)) : 500;
    return [entry.path, Math.min(entry.lines, size > 500 ? size : 500)];
  }));
  return { origin, ratchetBase, before, historical, ceilings };
}
