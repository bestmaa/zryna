// Versioned complete M6 evidence inventory; entries are obligations, not passing results.
function freeze(value) {
  if (value && typeof value === 'object') {
    Object.values(value).forEach(freeze);
    Object.freeze(value);
  }
  return value;
}

const artifactRoles = {
  project: ['compiler-release', 'node-runtime', 'provider'],
  lsp: ['compiler-release', 'node-runtime', 'provider', 'server-platform'],
  formatter: ['compiler-release', 'node-runtime', 'provider'],
  extension: ['compiler-release', 'node-runtime', 'provider', 'extension', 'server-platform'],
  setup: ['setup-platform', 'extension', 'server-platform'],
  playground: ['playground-toolkit', 'playground-compiler', 'node-runtime', 'provider'],
  transport: ['playground-toolkit'],
  containment: ['playground-toolkit', 'playground-compiler', 'node-runtime', 'provider',
    'bubblewrap-linux', 'python-linux', 'host-policy'],
  browser: ['playground-toolkit', 'chrome-linux', 'host-policy'],
  accessibility: ['playground-toolkit', 'chrome-linux', 'host-policy'],
  documentation: ['docs-bundle', 'compiler-release', 'server-platform', 'extension', 'setup-platform'],
  publication: ['playground-toolkit', 'compiler-release', 'docs-bundle'],
  canonical: ['compiler-release', 'playground-compiler', 'server-platform', 'extension', 'setup-platform'],
  hosted: ['compiler-release', 'playground-compiler', 'server-platform', 'extension', 'setup-platform'],
};

export const registry = freeze({ format: 'zryna.m6-registry.v1', version: 1,
  products: { compiler: '0.2.3', server: '0.5.0', extension: '0.5.0',
    setup: '0.1.0-candidate.3', playground: '0.1.0', chrome: '153.0.8010.12' },
  requiredArtifacts: ['compiler-release', 'node-runtime', 'provider', 'server-linux', 'server-windows',
    'extension', 'setup-linux', 'setup-windows', 'playground-toolkit', 'playground-compiler',
    'chrome-linux', 'bubblewrap-linux', 'python-linux', 'host-policy', 'docs-bundle'],
  gates: [
    { id: 'project', platforms: ['linux', 'windows'], cases: ['creation', 'unsafe-path', 'collision',
      'retained-root', 'frozen-source', 'javascript', 'webassembly', 'forbidden-override'] },
    { id: 'lsp', platforms: ['linux', 'windows'], cases: ['framing', 'open-change-close', 'unicode-crlf',
      'compiler-diagnostics', 'cancel-stale', 'unsupported-method', 'scalar-definition'] },
    { id: 'formatter', platforms: ['linux', 'windows'], cases: ['scalar-document', 'scalar-range',
      'm2-document', 'm2-range', 'm3-document', 'm3-range', 'idempotence', 'token-comment-preservation',
      'rejected-source', 'saved-import', 'cancel-stale', 'limits'] },
    { id: 'extension', platforms: ['linux', 'windows'], cases: ['vsix-identity', 'pre-source-handshake',
      'workspace-trust', 'profile-selection', 'diagnostics', 'definition', 'formatting',
      'scalar-run', 'm2-run', 'saved-file', 'root-substitution', 'cleanup'] },
    { id: 'setup', platforms: ['linux', 'windows'], cases: ['inventory', 'relocation', 'tamper',
      'compatibility', 'reproduction', 'project-editor-exercise'] },
    { id: 'playground', platforms: ['linux'], cases: ['corpus-cli-parity', 'editable-source',
      'actual-diagnostics', 'scalar-observations', 'export-arity', 'i32-boundaries', 'malformed',
      'source-budget', 'request-budget', 'frame-budget', 'unsupported', 'expired',
      'revision-cancellation', 'single-admission', 'authority-substitution', 'inert-text', 'recovery'] },
    { id: 'transport', platforms: ['linux', 'windows'], cases: ['closed-carriers', 'byte-boundaries',
      'revision-identity', 'late-upload', 'overlapping-finish', 'failed-teardown', 'unsupported-host'] },
    { id: 'containment', platforms: ['linux'], cases: ['nonroot-host', 'helper-identity',
      'sealed-materials', 'namespaces', 'seccomp-denials', 'no-ambient-authority', 'process-tree',
      'membership-before-source', 'memory', 'tasks', 'cpu', 'scratch', 'output', 'compile-deadline',
      'cancel-deadline', 'reap-cleanup', 'acquisition-faults', 'recovery'] },
    { id: 'browser', platforms: ['linux'], cases: ['archive-identity', 'fresh-profile', 'cgroup-baseline',
      'zero-imports', 'no-linear-memory', 'external-watchdog', 'zero-worker', 'evaluation-deadline',
      'cancellation', 'memory', 'tasks', 'cpu', 'session-isolation', 'recovery'] },
    { id: 'accessibility', platforms: ['linux'], cases: ['keyboard', 'labels-status',
      'focus-invalid-input', 'unicode-navigation', 'accessibility-tree'] },
    { id: 'documentation', platforms: ['linux', 'windows'], cases: ['support-matrix',
      'export-identity', 'runnable-project-editor-exercises'] },
    { id: 'publication', platforms: ['linux'], cases: ['immutable-artifacts', 'outer-signature',
      'nested-signature', 'independent-download', 'runnable-playground-exercise'] },
    { id: 'canonical', platforms: ['linux', 'windows'], cases: ['architecture', 'format', 'clippy',
      'workspace-tests', 'rustdoc', 'adapter', 'protocol', 'm0', 'm2', 'm3', 'tooling'] },
    { id: 'hosted', platforms: ['linux', 'windows'], cases: ['required-jobs', 'exact-source',
      'no-required-skips', 'retained-evidence'] },
  ].map(gate => ({ ...gate, artifactRoles: artifactRoles[gate.id] })) });

export const requiredGates = Object.freeze(registry.gates.flatMap(gate =>
  gate.platforms.map(platform => `${gate.id}-${platform}`)));
