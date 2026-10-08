import { MODULE_LIMIT, POLICY_PATH, fail, physicalLines, validatePolicy } from './policy.mjs';
import { blobs, commit, git, renames, source, tree } from './repository.mjs';
import { moduleLines } from './module-size.mjs';

function policyAt(root, revision) {
  const entry = tree(root, revision).get(POLICY_PATH);
  if (!entry) return undefined;
  if (entry.mode !== '100644') fail('module history policy is not a regular file');
  return validatePolicy(JSON.parse(blobs(root, [entry]).get(entry.hash)), '0000-00-00');
}

export function moduleInventory(root, revision, classifications) {
  const entries = [...tree(root, revision)].filter(([path, entry]) => source(path)
    && ['100644', '100755'].includes(entry.mode));
  const contents = blobs(root, entries.map(([, entry]) => entry));
  const excluded = new Set(classifications.map(entry => entry.path));
  return entries.map(([path, entry]) => ({ path, lines: excluded.has(path)
    ? physicalLines(contents.get(entry.hash)) : moduleLines(path, contents.get(entry.hash)),
    production: !excluded.has(path) })).filter(entry => entry.lines > MODULE_LIMIT);
}

// The first v2 policy authenticates the reviewed migration even after squash adoption.
// Its source bytes may lower ceilings; neither later policies nor copies may raise them.
export function moduleHistory(root, comparison, head, policy, trustedPolicy, bootstrap) {
  if (policy.version !== 2) fail('working policy must use the hard 300-line module contract (version 2)');
  let origin = policy.modules.anchor;
  if (trustedPolicy?.version === 2) {
    if (JSON.stringify(policy.modules) !== JSON.stringify(trustedPolicy.modules)) {
      fail('module baseline or anchor differs from trusted base policy');
    }
    const revisions = git(root, ['log', '--format=%H', '--reverse', comparison, '--', POLICY_PATH])
      .trim().split('\n').filter(Boolean);
    let adopted;
    for (const revision of revisions) {
      const previous = policyAt(root, revision);
      if (previous?.version === 2) {
        if (!adopted) { adopted = previous; origin = revision; }
        if (JSON.stringify(previous.modules) !== JSON.stringify(adopted.modules)) {
          fail('module baseline differs from original trusted module adoption');
        }
      } else if (adopted) fail('module policy adoption history has a deletion or downgrade');
    }
    if (!adopted || JSON.stringify(adopted.modules) !== JSON.stringify(policy.modules)) {
      fail('module baseline differs from original trusted module adoption');
    }
  } else {
    if (origin !== bootstrap) fail('module anchor differs from the reviewed migration commit');
    commit(root, origin);
    git(root, ['merge-base', '--is-ancestor', origin, head]);
    const common = git(root, ['merge-base', origin, comparison]).trim();
    if (common !== origin && common !== comparison) fail('module migration and trusted base are incomparable');
    const anchored = policyAt(root, origin);
    const expected = moduleInventory(root, origin, anchored?.classifications ?? policy.classifications);
    if (JSON.stringify(policy.modules.baseline) !== JSON.stringify(expected)) {
      fail('module baseline differs from exact anchored inventory; do not raise or regenerate allowances');
    }
  }
  const entries = tree(root, origin);
  const contents = blobs(root, [...entries].filter(([path, entry]) => source(path)
    && ['100644', '100755'].includes(entry.mode)).map(([, entry]) => entry));
  const ceilings = new Map(policy.modules.baseline.filter(entry => entry.production).map(entry => {
    const adopted = entries.get(entry.path);
    const size = adopted && contents.has(adopted.hash) ? moduleLines(entry.path, contents.get(adopted.hash)) : MODULE_LIMIT;
    return [entry.path, Math.min(entry.lines, Math.max(MODULE_LIMIT, size))];
  }));
  return { ceilings, renames: renames(root, origin, comparison) };
}
