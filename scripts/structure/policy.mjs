export const POLICY_PATH = 'scripts/repository-structure-policy.json';
export const INITIAL_COMMIT = '885bb4d863ad112566add72fab6d2931587b71b1';
export const MODULE_COMMIT = '0635c19f922f7af61fa1e05b1a632b1013e908e7';
export const MODULE_LIMIT = 300;
export const LEGACY_SOURCE_EXTENSIONS = ['.rs', '.mjs', '.js', '.cjs', '.ts', '.sh', '.ps1', '.py'];
export const SOURCE_EXTENSIONS = [...LEGACY_SOURCE_EXTENSIONS,
  '.jsx', '.tsx', '.mts', '.cts', '.c', '.h', '.cc', '.cpp', '.hpp'];

export function fail(message) { throw new Error(`structure: ${message}`); }

export function physicalLines(text) {
  if (text.length === 0) return 0;
  return (text.match(/\n/g)?.length ?? 0) + (text.endsWith('\n') ? 0 : 1);
}

export function portablePath(value) {
  if (typeof value !== 'string' || !/^[A-Za-z0-9_.\/-]+$/.test(value)
      || value.startsWith('/') || value.split('/').some(part => !part || part === '.' || part === '..'
        || /[. ]$/.test(part) || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part))) {
    fail(`invalid exact path: ${JSON.stringify(value)}`);
  }
  return value;
}

function keys(record, expected, label) {
  if (!record || typeof record !== 'object' || Array.isArray(record)
      || Object.keys(record).sort().join() !== [...expected].sort().join()) fail(`invalid ${label} fields`);
}

function text(value, label) {
  if (typeof value !== 'string' || value.trim().length < 3) fail(`missing ${label}`);
}

export function validatePolicy(policy, today) {
  keys(policy, ['version', 'anchor', 'baseline', 'classifications', 'exceptions',
    ...(policy.version === 2 ? ['modules'] : [])], 'policy');
  if (![1, 2].includes(policy.version) || !/^[a-f0-9]{40}$/.test(policy.anchor)) fail('invalid version or anchor commit');
  if (policy.version === 2) {
    keys(policy.modules, ['anchor', 'baseline'], 'module migration');
    if (!/^[a-f0-9]{40}$/.test(policy.modules.anchor)) fail('invalid module migration anchor');
    validateInventory(policy.modules.baseline, MODULE_LIMIT, 'module baseline');
  }
  for (const group of ['baseline', 'classifications', 'exceptions']) {
    if (!Array.isArray(policy[group])) fail(`${group} must be an array`);
    const seen = new Set();
    for (const entry of policy[group]) {
      const expected = group === 'baseline' ? ['path', 'lines', 'production'] : group === 'classifications'
        ? ['path', 'kind', 'owner', 'reason', 'review']
        : ['path', 'owner', 'reason', 'ceiling', 'review', 'expires'];
      keys(entry, expected, group);
      portablePath(entry.path);
      if (seen.has(entry.path.toLowerCase())) fail(`duplicate ${group} path: ${entry.path}`);
      seen.add(entry.path.toLowerCase());
      if (group === 'baseline') {
        if (!Number.isSafeInteger(entry.lines) || entry.lines <= 500) fail(`invalid baseline count: ${entry.path}`);
        if (typeof entry.production !== 'boolean') fail(`invalid initial production classification: ${entry.path}`);
      } else {
        for (const field of ['owner', 'reason', 'review']) text(entry[field], `${group} ${field}`);
        if (group === 'classifications' && !['test', 'fixture', 'generated'].includes(entry.kind)) fail(`invalid classification: ${entry.path}`);
        if (group === 'exceptions') {
          if (!Number.isSafeInteger(entry.ceiling) || entry.ceiling <= 500) fail(`invalid exception ceiling: ${entry.path}`);
          if (!/^\d{4}-\d{2}-\d{2}$/.test(entry.expires)
              || !Number.isFinite(Date.parse(`${entry.expires}T00:00:00Z`))
              || new Date(`${entry.expires}T00:00:00Z`).toISOString().slice(0, 10) !== entry.expires) fail(`invalid UTC expiry: ${entry.path}`);
          if (entry.expires <= today) fail(`expired exception: ${entry.path} (expires at start of ${entry.expires} UTC)`);
        }
      }
    }
  }
  return policy;
}

function validateInventory(inventory, limit, label) {
  if (!Array.isArray(inventory)) fail(`${label} must be an array`);
  const seen = new Set();
  for (const entry of inventory) {
    keys(entry, ['path', 'lines', 'production'], label);
    portablePath(entry.path);
    if (seen.has(entry.path.toLowerCase())) fail(`duplicate ${label} path: ${entry.path}`);
    seen.add(entry.path.toLowerCase());
    if (!Number.isSafeInteger(entry.lines) || entry.lines <= limit) fail(`invalid ${label} count: ${entry.path}`);
    if (typeof entry.production !== 'boolean') fail(`invalid ${label} production classification: ${entry.path}`);
  }
}

export function reviewChanges(previous, current) {
  const messages = [];
  if (JSON.stringify(previous?.modules) !== JSON.stringify(current.modules)) {
    messages.push('REVIEW module migration changed: the anchored inventory must be authenticated, never regenerated or raised');
  }
  for (const group of ['baseline', 'classifications', 'exceptions']) {
    const before = new Map((previous?.[group] ?? []).map(entry => [entry.path, entry]));
    const after = new Map(current[group].map(entry => [entry.path, entry]));
    for (const path of [...new Set([...before.keys(), ...after.keys()])].sort()) {
      if (JSON.stringify(before.get(path)) === JSON.stringify(after.get(path))) continue;
      messages.push(`REVIEW ${group} ${path}: ${JSON.stringify(before.get(path) ?? null)} -> ${JSON.stringify(after.get(path) ?? null)}`);
    }
  }
  return messages;
}
