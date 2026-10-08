import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, renameSync, symlinkSync, unlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { checkRepository } from '../scripts/check-repository-structure.mjs';
import { moduleLines } from '../scripts/structure/module-size.mjs';
import { moduleInventory } from '../scripts/structure/module-history.mjs';
import { physicalLines, POLICY_PATH, validatePolicy } from '../scripts/structure/policy.mjs';
import { git } from '../scripts/structure/repository.mjs';
import { validateUnsafeRustDocuments } from '../scripts/structure/unsafe-rust.mjs';
import { PREFLIGHT_COMMANDS, validatePreflightCommands } from '../scripts/run-preflight.mjs';
import { validateManifestDocument, validatePackageDocument } from '../scripts/run-m0-conformance.mjs';

const today = '2026-09-06';
const lines = count => '// source\n'.repeat(count);
const metadata = { owner: 'compiler maintainers', reason: 'One cohesive reviewed responsibility', review: 'https://github.com/zryna/zryna/issues/314' };

function unsafeRustDocuments() {
  const rust = `unsafe_code = "forbid"\nmissing_docs = "warn"\nrust_2018_idioms = { level = "deny", priority = -1 }\nunused_lifetimes = "deny"\nunused_qualifications = "deny"`;
  const clippy = `all = { level = "deny", priority = -1 }\npedantic = { level = "warn", priority = -1 }\ndbg_macro = "deny"\ntodo = "deny"\nunimplemented = "deny"\nunwrap_used = "deny"`;
  return new Map([
    ['Cargo.toml', `[workspace.lints.rust]\n${rust}\n\n[workspace.lints.clippy]\n${clippy}\n`],
    ['zryna.workspace.json', JSON.stringify({ members: [
      { id: 'safe', root: 'crates/safe' },
      { id: 'zryna-windows-filesystem', root: 'crates/zryna-windows-filesystem' },
    ] })],
    ['crates/safe/Cargo.toml', '[lints]\nworkspace = true\n'],
    ['crates/safe/src/lib.rs', 'pub fn safe() {}\n'],
    ['crates/zryna-windows-filesystem/Cargo.toml', `[lints.rust]\n${rust.replace('"forbid"', '"deny"')}\n\n[lints.clippy]\n${clippy}\n`],
    ['crates/zryna-windows-filesystem/src/windows.rs', '#![allow(unsafe_code)]\nunsafe fn syscall() {}\n'],
  ]);
}

test('unsafe exception rejects a second component lint override', () => {
  for (const manifest of ['crates/safe/Cargo.toml', 'crates/unregistered/Cargo.toml']) {
    const files = unsafeRustDocuments();
    files.set(manifest, '[lints]\nworkspace = true\n\n[lints.rust]\nunsafe_code = "allow"\n');
    assert.throws(() => validateUnsafeRustDocuments(files), /inherit workspace lints/);
  }
});

test('unsafe exception rejects allowances and unsafe code outside the exact module', () => {
  for (const [path, source] of [
    ['crates/safe/src/lib.rs', '#![allow(unsafe_code)]\n'],
    ['crates/safe/src/lib.rs', '#![cfg_attr(any(), allow(/* split */ unsafe_code))]\n'],
    ['crates/safe/src/lib.rs', 'unsafe/* split */{}\n'],
    ['crates/safe/src/lib.rs', '#[unsafe(no_mangle)]\npub extern "C" fn escaped() {}\n'],
    ['crates/safe/src/lib.rs', '#[cfg(any())]\nunsafe fn disabled() {}\n'],
    ['crates/safe/src/HIDDEN.RS', 'unsafe fn uppercase_extension() {}\n'],
    ['unreferenced/unused.rs', 'unsafe fn unreferenced() {}\n'],
  ]) {
    const files = unsafeRustDocuments();
    files.set(path, source);
    assert.throws(() => validateUnsafeRustDocuments(files), /unsafe Rust is confined/);
  }
});

test('unsafe lexical inspection ignores comments and every Rust literal form', () => {
  const files = unsafeRustDocuments();
  files.set('crates/safe/src/lib.rs', String.raw`
// unsafe { #[allow(unsafe_code)] }
/* outer unsafe { /* nested unsafe fn hidden() {} */ } */
pub const TEXT: &str = "unsafe { #[unsafe(no_mangle)]";
pub const BYTE_TEXT: &[u8] = b"unsafe {";
pub const RAW: &str = r###"unsafe { #[allow(unsafe_code)]"###;
pub const RAW_BYTES: &[u8] = br##"unsafe fn hidden() {}"##;
pub const CHARACTER: char = '{';
pub const ESCAPED: char = '\x7b';
pub const UNICODE_ESCAPE: char = '\u{7b}';
pub const QUOTE: char = '\'';
pub const BYTE: u8 = b'{';
pub const BYTE_ESCAPE: u8 = b'\x7b';
pub fn retained<'a>(value: &'a str) -> &'a str { value }
`);
  assert.doesNotThrow(() => validateUnsafeRustDocuments(files));
});

test('lint manifests reject spaced, dotted, and quoted override spellings', () => {
  for (const document of [
    '[lints]\nworkspace = true\n\n[lints . rust]\nunsafe_code = "allow"\n',
    '[lints]\nworkspace = true\n\n["lints"."rust"]\nunsafe_code = "allow"\n',
    'lints.rust.unsafe_code = "allow"\n',
    '[lints]\nworkspace = true\nrust . unsafe_code = "allow"\n',
    '[ "lints" ]\nworkspace = true\n',
  ]) {
    const files = unsafeRustDocuments();
    files.set('crates/safe/Cargo.toml', document);
    assert.throws(() => validateUnsafeRustDocuments(files), /canonical|inherit/);
  }
});

test('unsafe exception preserves workspace forbid, inherited defaults, and copied lints', () => {
  for (const mutate of [
    files => files.set('Cargo.toml', files.get('Cargo.toml').replace('unsafe_code = "forbid"', 'unsafe_code = "allow"')),
    files => files.set('crates/safe/Cargo.toml', '[lints]\n'),
    files => files.set('crates/zryna-windows-filesystem/Cargo.toml', files.get('crates/zryna-windows-filesystem/Cargo.toml').replace('todo = "deny"\n', '')),
  ]) {
    const files = unsafeRustDocuments();
    mutate(files);
    assert.throws(() => validateUnsafeRustDocuments(files));
  }
});

test('frozen preflight and M0 declarations cannot silently omit structure enforcement', () => {
  assert.equal(PREFLIGHT_COMMANDS[0].id, 'repository-structure');
  assert.throws(() => validatePreflightCommands(PREFLIGHT_COMMANDS.slice(1)), /frozen/);
  const manifest = JSON.parse(readFileSync(new URL('./m0-conformance-v1.json', import.meta.url)));
  assert.doesNotThrow(() => validateManifestDocument(manifest));
  for (const id of ['repository-structure', 'repository-structure-tests']) {
    const changed = structuredClone(manifest);
    changed.commands = changed.commands.filter(command => command.id !== id);
    assert.throws(() => validateManifestDocument(changed));
  }
  const pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url)));
  delete pkg.scripts['structure:check'];
  assert.throws(() => validatePackageDocument(pkg), /structure:check/);
  const workflow = readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8');
  assert.match(workflow, /ZRYNA_STRUCTURE_BASE:.*pull_request.base.sha.*inputs.structure_base/);
  assert.match(workflow, /os: \[ubuntu-latest, windows-latest\]/);
  assert.equal((workflow.match(/uses: actions\/checkout@/g) ?? []).length,
    (workflow.match(/fetch-depth: 0/g) ?? []).length);
});
function fixture(t, count = 600, testOnly = false) {
  const root = mkdtempSync(join(tmpdir(), 'zryna-structure-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const write = (path, content) => {
    mkdirSync(join(root, path, '..'), { recursive: true });
    writeFileSync(join(root, path), content);
  };
  git(root, ['init', '-q']);
  git(root, ['config', 'user.name', 'Structure test']);
  git(root, ['config', 'user.email', 'structure@example.invalid']);
  git(root, ['config', 'core.autocrlf', 'false']);
  const save = () => { git(root, ['add', '.']); git(root, ['commit', '-qm', 'Checkpoint']); return git(root, ['rev-parse', 'HEAD']).trim(); };
  write('src/main.rs', lines(count));
  write('docs/CODE_NAVIGATION.md', '[source](../src/main.rs)\n');
  const anchor = save();
  const policy = { version: 2, modules: { anchor, baseline: count > 300 ? [{ path: 'src/main.rs', lines: count, production: !testOnly }] : [] }, anchor, baseline: count > 500 ? [{ path: 'src/main.rs', lines: count, production: !testOnly }] : [], classifications: testOnly ? [{ path: 'src/main.rs', kind: 'test', ...metadata }] : [], exceptions: [] };
  const savePolicy = () => write(POLICY_PATH, `${JSON.stringify(policy, null, 2)}\n`);
  savePolicy();
  const initial = save();
  const check = options => checkRepository({ root, today, bootstrap: anchor, moduleBootstrap: anchor, base: 'HEAD', ...options });
  return { root, write, save, policy, savePolicy, check, anchor, initial };
}

test('physical counts are portable at exact hard boundaries', () => {
  assert.equal(physicalLines(''), 0);
  assert.equal(physicalLines('text'), 1);
  assert.equal(physicalLines('\r\n'), 1);
  for (const count of [299, 300, 301]) {
    assert.equal(physicalLines(lines(count)), count);
    assert.equal(physicalLines(lines(count).replaceAll('\n', '\r\n')), count);
    assert.equal(physicalLines(lines(count).slice(0, -1)), count);
  }
});

test('new files accept 300 and reject 301, including nested fixture disguises', t => {
  const f = fixture(t);
  for (const count of [299, 300, 301]) {
    f.write('src/fixtures/production.rs', lines(count));
    const result = f.check();
    assert.equal(result.ok, count <= 300);
    if (count > 300) assert.match(result.messages.join('\n'), /src\/fixtures\/production.rs: 301 module lines \(301 physical\) exceeds 300; split by cohesive responsibility/);
  }
  f.write('src/fixtures/production.rs', lines(100));
  assert.equal(f.check().ok, true);
  f.write('.gitignore', 'ignored.rs\n');
  f.write('ignored.rs', lines(301));
  assert.equal(f.check().ok, false);
});

test('compressed JS/TS cannot hide 301 printed statements behind one physical line', t => {
  const f = fixture(t, 100);
  for (const extension of ['mjs', 'cjs', 'js', 'ts', 'jsx', 'tsx', 'mts', 'cts']) {
    const path = `src/nested/production.${extension}`;
    const content = count => Array.from({ length: count }, (_, i) => `const value${i} = ${i};`).join('');
    f.write(path, content(300));
    assert.equal(moduleLines(path, content(300)), 300);
    assert.equal(f.check().ok, true);
    f.write(path, content(301));
    assert.match(f.check().messages.join('\n'), /301 module lines \(1 physical\) exceeds 300/);
    rmSync(join(f.root, path));
  }
  assert.throws(() => moduleLines('invalid.mjs', 'export {'), /cannot normalize invalid.mjs/);
  assert.equal(moduleLines('blank.mjs', '\n'.repeat(301)), 301);
  f.write('program.zry', lines(1000));
  assert.equal(f.check().ok, true);
});

test('comma-separated bindings and calls obey meaningful 300/301 boundaries', t => {
  const f = fixture(t, 100);
  for (const content of [
    count => `const ${Array.from({ length: count }, (_, i) => `value${i}=${i}`).join(',')};`,
    count => `${Array.from({ length: count }, (_, i) => `run(${i})`).join(',')};`,
    count => `const {${Array.from({ length: count }, (_, i) => `property${i}:value${i}`).join(',')}}=input;`,
  ]) {
    f.write('src/compressed.mjs', content(300));
    assert.equal(moduleLines('src/compressed.mjs', content(300)), 300);
    assert.equal(f.check().ok, true);
    f.write('src/compressed.mjs', content(301));
    assert.match(f.check().messages.join('\n'), /301 module lines.*exceeds 300/);
  }
});

test('C/C++ production and header files are included at 300/301', t => {
  const f = fixture(t, 100);
  for (const extension of ['c', 'h', 'cc', 'cpp', 'hpp']) {
    const path = `runtime/nested/production.${extension}`;
    f.write(path, lines(300));
    assert.equal(f.check().ok, true);
    f.write(path, lines(301));
    assert.match(f.check().messages.join('\n'), /301 module lines.*exceeds 300/);
    rmSync(join(f.root, path));
  }
});

test('301–500 legacy modules have fixed ceilings, ratchet to 300 and never cover copies', t => {
  const f = fixture(t, 400);
  assert.equal(f.check().ok, true);
  f.write('src/main.rs', lines(401));
  assert.match(f.check().messages.join('\n'), /exceeds 400/);
  f.write('src/main.rs', lines(350));
  f.save();
  f.write('src/main.rs', lines(351));
  assert.match(f.check().messages.join('\n'), /exceeds 350/);
  f.write('src/main.rs', lines(350));
  git(f.root, ['mv', 'src/main.rs', 'src/moved.rs']);
  f.write('docs/CODE_NAVIGATION.md', '[source](../src/moved.rs)\n');
  assert.equal(f.check().ok, true);
  f.write('src/copied.rs', lines(350));
  assert.match(f.check().messages.join('\n'), /src\/copied.rs: 350 module lines.*exceeds 300/);
  rmSync(join(f.root, 'src/copied.rs'));
  f.write('src/moved.rs', lines(300));
  f.save();
  f.write('src/moved.rs', lines(301));
  assert.match(f.check().messages.join('\n'), /exceeds 300/);
});

test('module inventories reject raised ceilings, changed eligibility, omissions and substituted anchors', t => {
  for (const mutate of [
    p => { p.modules.baseline[0].lines = 401; },
    p => { p.modules.baseline[0].production = false; },
    p => { p.modules.baseline = []; },
    p => { p.modules.anchor = 'a'.repeat(40); },
  ]) {
    const f = fixture(t, 400);
    mutate(f.policy);
    f.savePolicy();
    assert.throws(() => f.check(), /module baseline or anchor differs/);
    f.save();
    assert.throws(() => f.check(), /original trusted module adoption/);
  }
});

test('v1 to v2 migration reproduces the exact inventory from the pinned Git object', t => {
  const f = fixture(t, 400);
  const old = structuredClone(f.policy);
  old.version = 1;
  delete old.modules;
  f.write(POLICY_PATH, JSON.stringify(old));
  const migration = f.save();
  f.policy.modules = { anchor: migration, baseline: moduleInventory(f.root, migration, old.classifications) };
  f.savePolicy();
  const check = () => f.check({ moduleBootstrap: migration });
  assert.equal(check().ok, true);
  f.policy.modules.baseline[0].lines = 401;
  f.savePolicy();
  assert.throws(check, /exact anchored inventory/);
  f.policy.modules.baseline[0].lines = 400;
  f.policy.version = 1;
  delete f.policy.modules;
  f.savePolicy();
  assert.throws(check, /hard 300-line module contract/);
});

test('trusted reductions ratchet; working tree, index and committed base cannot raise the ceiling', t => {
  const f = fixture(t);
  assert.equal(f.check().ok, true);
  f.write('src/main.rs', lines(601));
  assert.equal(f.check().ok, false);
  git(f.root, ['add', 'src/main.rs']);
  assert.equal(f.check().ok, false);
  f.write('src/main.rs', lines(550));
  assert.equal(f.check().ok, true);
  f.save();
  f.write('src/main.rs', lines(551));
  assert.match(f.check().messages.join('\n'), /exceeds 550/);
  f.write('src/main.rs', lines(300));
  f.save();
  f.write('src/main.rs', lines(301));
  assert.match(f.check().messages.join('\n'), /exceeds 300/);
  f.policy.baseline[0].lines = 700;
  f.savePolicy();
  assert.throws(() => f.check(), /baseline differs/);
});

test('renames preserve grandfathering and reductions; deletion keeps historical inventory valid', t => {
  const f = fixture(t);
  git(f.root, ['mv', 'src/main.rs', 'src/renamed.rs']);
  f.write('docs/CODE_NAVIGATION.md', '[source](../src/renamed.rs)\n');
  assert.equal(f.check().ok, true);
  f.write('src/renamed.rs', lines(550));
  f.save();
  f.write('src/renamed.rs', lines(551));
  assert.match(f.check().messages.join('\n'), /exceeds 550/);
  rmSync(join(f.root, 'src/renamed.rs'));
  f.write('docs/CODE_NAVIGATION.md', '[policy](../scripts/repository-structure-policy.json)\n');
  assert.equal(f.check().ok, true);
});

test('physical exceptions require exact metadata and cannot bypass the hard module limit', t => {
  const f = fixture(t, 100);
  f.write('src/main.rs', lines(501));
  f.policy.exceptions.push({ path: 'src/main.rs', ...metadata, ceiling: 501, expires: '2026-09-07' });
  f.savePolicy();
  assert.equal(f.check().ok, false);
  assert.match(f.check().messages.join('\n'), /exceeds 300/);
  assert.match(f.check().messages.join('\n'), /explicit maintainer review/);
  f.write('src/main.rs', lines(502));
  assert.equal(f.check().ok, false);
  for (const mutate of [
    p => { p.exceptions[0].expires = today; },
    p => { p.exceptions[0].expires = '2026-02-30'; },
    p => { p.exceptions[0].path = 'src/*'; },
    p => { delete p.exceptions[0].owner; },
    p => { p.exceptions[0].reason = ''; },
    p => { p.exceptions[0].ceiling = '700'; },
    p => p.exceptions.push({ ...p.exceptions[0] }),
  ]) {
    const changed = structuredClone(f.policy);
    mutate(changed);
    assert.throws(() => validatePolicy(changed, today));
  }
  f.write('src/main.rs', lines(500));
  assert.throws(() => f.check(), /stale exception/);
});

test('test and generated exemptions are exact reviewed records, with stale paths rejected', t => {
  const f = fixture(t, 100);
  for (const kind of ['test', 'fixture', 'generated']) {
    f.write('tests/large.rs', lines(501));
    assert.equal(f.check().ok, false);
    f.policy.classifications = [{ path: 'tests/large.rs', kind, ...metadata }];
    f.savePolicy();
    assert.equal(f.check().ok, true);
    f.write('tests/unlisted.rs', '// generated\n' + lines(501));
    assert.equal(f.check().ok, false);
    rmSync(join(f.root, 'tests/unlisted.rs'));
    f.policy.classifications = [];
    f.savePolicy();
  }
  f.policy.classifications = [{ path: 'missing.rs', kind: 'test', ...metadata }];
  f.savePolicy();
  assert.throws(() => f.check(), /stale policy path/);
});

test('initial test-only source never acquires grandfathering after reclassification', t => {
  const f = fixture(t, 600, true);
  assert.equal(f.check().ok, true);
  f.policy.classifications = [];
  f.savePolicy();
  assert.equal(f.check().ok, false);
  f.policy.baseline[0].production = true;
  f.savePolicy();
  assert.throws(() => f.check(), /baseline differs/);
});

test('missing or ambiguous authority fails closed; linked worktrees and explicit PR bases work', t => {
  const f = fixture(t);
  assert.throws(() => f.check({ base: 'main' }), /full immutable/);
  assert.throws(() => f.check({ base: 'a'.repeat(40) }), /authority unavailable/);
  const linked = join(f.root, '..', `${f.root.split(/[\\/]/).at(-1)}-linked`);
  git(f.root, ['worktree', 'add', '--detach', linked, 'HEAD']);
  t.after(() => rmSync(linked, { recursive: true, force: true }));
  assert.equal(checkRepository({ root: linked, base: f.initial, today }).ok, true);
  f.write('src/main.rs', lines(550));
  f.save();
  assert.equal(f.check({ base: f.initial }).ok, true);
});

test('shallow clone without required anchor rejects rather than skipping history', t => {
  const f = fixture(t);
  const shallow = join(f.root, 'shallow');
  git(f.root, ['clone', '--depth=1', '--no-local', f.root, shallow]);
  assert.throws(() => checkRepository({ root: shallow, base: 'HEAD', today }), /authority unavailable/);
});

test('unsafe paths, portable case collisions, symlink ancestors and broken navigation fail', t => {
  const f = fixture(t);
  f.policy.classifications.push({ path: '../outside.rs', kind: 'test', ...metadata });
  f.savePolicy();
  assert.throws(() => f.check(), /invalid exact path/);
  f.policy.classifications = [];
  f.savePolicy();
  const hash = git(f.root, ['rev-parse', 'HEAD:src/main.rs']).trim();
  git(f.root, ['update-index', '--add', '--cacheinfo', `100644,${hash},src/Main.rs`]);
  assert.throws(() => f.check(), /case-colliding/);
  git(f.root, ['update-index', '--force-remove', 'src/Main.rs']);
  renameSync(join(f.root, 'src'), join(f.root, 'real'));
  symlinkSync(join(f.root, 'real'), join(f.root, 'src'), process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => f.check(), /no-symlink/);
  unlinkSync(join(f.root, 'src'));
  renameSync(join(f.root, 'real'), join(f.root, 'src'));
  for (const target of ['../missing.rs', '../../outside.rs', '../src', 'https://example.invalid']) {
    f.write('docs/CODE_NAVIGATION.md', `[bad](${target})\n`);
    assert.throws(() => f.check());
  }
  f.write('docs/CODE_NAVIGATION.md', '[source](../src/main.rs)\n');
  assert.equal(f.check().ok, true);
});

test('diagnostics and recovery are deterministic and checker never mutates input', t => {
  const f = fixture(t);
  f.write('src/main.rs', lines(601));
  const status = git(f.root, ['status', '--porcelain=v1']);
  const policy = readFileSync(join(f.root, POLICY_PATH));
  const source = readFileSync(join(f.root, 'src/main.rs'));
  assert.deepEqual(f.check(), f.check());
  assert.equal(git(f.root, ['status', '--porcelain=v1']), status);
  assert.deepEqual(readFileSync(join(f.root, POLICY_PATH)), policy);
  assert.deepEqual(readFileSync(join(f.root, 'src/main.rs')), source);
  f.write('src/main.rs', lines(599));
  assert.equal(f.check().ok, true);
});

test('squashed adoption works in a fresh full clone without the bootstrap object', t => {
  const f = fixture(t);
  // The reviewed branch reduced the file before its policy was squash-adopted.
  f.write('src/main.rs', lines(550));
  f.save();
  const tree = git(f.root, ['rev-parse', 'HEAD^{tree}']).trim();
  const squash = git(f.root, ['commit-tree', tree, '-m', 'Squash adopted policy']).trim();
  git(f.root, ['checkout', '-qb', 'adopted', squash]);
  const cloned = join(f.root, '..', `${f.root.split(/[\\/]/).at(-1)}-clone`);
  t.after(() => rmSync(cloned, { recursive: true, force: true }));
  git(f.root, ['clone', '--no-local', '--single-branch', '--branch', 'adopted', f.root, cloned]);
  assert.throws(() => git(cloned, ['cat-file', '-e', f.anchor]), /authority unavailable/);
  const check = () => checkRepository({ root: cloned, base: squash, today });
  assert.equal(check().ok, true);
  assert.deepEqual(check(), check());
  writeFileSync(join(cloned, 'src/main.rs'), lines(551));
  assert.match(check().messages.join('\n'), /exceeds 550/);
  writeFileSync(join(cloned, 'src/main.rs'), lines(550));
  git(cloned, ['mv', 'src/main.rs', 'src/renamed.rs']);
  writeFileSync(join(cloned, 'docs/CODE_NAVIGATION.md'), '[source](../src/renamed.rs)\n');
  assert.equal(check().ok, true);
  writeFileSync(join(cloned, 'src/renamed.rs'), lines(551));
  assert.match(check().messages.join('\n'), /exceeds 550/);
  writeFileSync(join(cloned, 'src/renamed.rs'), lines(550));
  assert.equal(check().ok, true);
});

test('adopted inventory and anchor cannot be changed even in a later comparison commit', t => {
  for (const mutate of [
    p => { p.baseline[0].lines = 601; },
    p => { p.baseline[0].production = false; },
    p => { p.anchor = 'a'.repeat(40); },
  ]) {
    const f = fixture(t);
    mutate(f.policy);
    f.savePolicy();
    assert.throws(() => f.check(), /baseline differs|anchor differs/);
    f.save();
    assert.throws(() => f.check(), /differs from original trusted policy adoption/);
  }
});

test('bootstrap still authenticates exact inventory and rejects invented anchors', t => {
  const f = fixture(t);
  assert.equal(f.check({ base: f.anchor }).ok, true);
  f.policy.baseline[0].lines = 601;
  f.savePolicy();
  assert.throws(() => f.check({ base: f.anchor }), /exact anchored source inventory/);
  f.policy.baseline[0].lines = 600;
  f.policy.anchor = 'a'.repeat(40);
  f.savePolicy();
  assert.throws(() => f.check({ base: f.anchor }), /anchor differs/);
});

test('deleted and reintroduced policy has ambiguous adoption and cannot reset ceilings', t => {
  const f = fixture(t);
  unlinkSync(join(f.root, POLICY_PATH));
  f.save();
  f.savePolicy();
  f.save();
  assert.throws(() => f.check(), /adoption history is absent or ambiguous/);
});
