import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const read = name => readFileSync(new URL(`../docs/${name}.md`, import.meta.url), 'utf8');
const normalized = text => text.replace(/\s+/g, ' ');

function checkReleaseBoundary(distribution, installation, roadmap) {
  assert.match(distribution, /Status: distribution contract implemented/);
  assert(normalized(distribution).includes('issues/424) remain open'));
  assert.match(installation, /Status: observed for the immutable `v0\.2\.3`/);
  assert.match(installation, /0decfd2056a77824003e386fcd78839b5b78d6b9/);
  assert.match(installation, /67f7d033003689a8cfcb7dc5691df86c85c9e603/);
  for (const [target, extension, digest] of [
    ['x86_64-pc-windows-msvc', 'zip', 'e34c2ea2c4cc0d4d53867926419ffe8fa9aa97254b80aea9df266b815bdd83fc'],
    ['x86_64-unknown-linux-gnu', 'tar.gz', 'b258d45e1eb58a701026ced7c74d03dc74c911c90a5ec7da93c656884a97159f'],
  ]) assert(installation.includes(`${digest}  zryna-0.2.3-${target}.${extension}`));
  assert.match(installation, /exactly `zryna 0\.2\.3`/);
  const upgrades = normalized(installation.split('## Upgrade proof boundary')[1] ?? '');
  assert(upgrades.includes('both hosts then observed `ZRYNA-P4009`'));
  assert(upgrades.includes('No canonical upgrade receipt was produced'));
  assert(upgrades.includes('`v0.2.1` to `v0.2.3` and `v0.2.2` to `v0.2.3`, on both supported public hosts'));
  assert(upgrades.includes("do not qualify the outer portable setup candidate or complete"));
  const beta = normalized(roadmap.split('## M9 — Downloadable Beta Distribution')[1]?.split('## M10')[0] ?? '');
  assert(beta.includes('external pilot and final beta record in #424 remain open'));
  assert(beta.includes('four public v0.2.3 upgrade paths'));
  assert(beta.includes('checkout M2/M3 comparison lane is not installed-distribution evidence'));
}

function checkPilotBoundary(pilot, setup) {
  const text = normalized(pilot);
  for (const fragment of [
    '**0.1.0-candidate.3**', 'compiler **0.2.3**',
    'language server **0.5.0**', 'editor **0.5.0**',
    'internal review candidate with production admission forbidden',
    'retained Windows candidate.2 invocation',
    'does not supply candidate.3 acceptance evidence',
    'M3 Run remains unavailable',
    'from scalar/M2 Run and from CLI rows',
    'not evidence that an external pilot or beta publication occurred',
    "does not satisfy Issue #424's external participant, clean-host, artifact identity or publication gates",
  ]) assert(text.includes(fragment), fragment);
  assert(normalized(setup).includes('**setup 0.1.0-candidate.3**'));
  assert(normalized(setup).includes('Explicit Run remains limited to scalar and M2'));
}

test('beta release docs retain completed upgrades and open external pilot gates', () => {
  checkReleaseBoundary(read('BETA_DISTRIBUTION'), read('BETA_INSTALLATION'), read('ROADMAP'));
});

test('pilot kit binds current setup while retaining historical and checkout evidence boundaries', () => {
  checkPilotBoundary(read('BETA_PILOT_KIT'), read('PORTABLE_SETUP'));
});

test('beta documentation regression rejects stale releases and broadened evidence claims', () => {
  const distribution = read('BETA_DISTRIBUTION');
  const installation = read('BETA_INSTALLATION');
  const roadmap = read('ROADMAP');
  const pilot = read('BETA_PILOT_KIT');
  const setup = read('PORTABLE_SETUP');
  for (const [before, after] of [
    ['exactly `zryna 0.2.3`', 'exactly `zryna 0.2.1`'],
    ['No canonical upgrade receipt was produced', 'A passing upgrade receipt was produced'],
    ['do not qualify the outer portable setup', 'qualify the outer portable setup'],
  ]) {
    assert(installation.includes(before));
    assert.throws(() => checkReleaseBoundary(distribution, installation.replace(before, after), roadmap));
  }
  assert.throws(() => checkReleaseBoundary(distribution, installation,
    roadmap.replace('final beta record in #424 remain open', 'final beta record in #424 is complete')));
  for (const [before, after] of [
    ['**0.1.0-candidate.3**', '**0.1.0-candidate.2**'],
    ['retained Windows candidate.2 invocation', 'retained Windows candidate.3 invocation'],
    ['M3 Run remains unavailable', 'M3 Run is supported'],
    ['production admission forbidden', 'production admission permitted'],
  ]) {
    assert(pilot.includes(before));
    assert.throws(() => checkPilotBoundary(pilot.replace(before, after), setup));
  }
});
