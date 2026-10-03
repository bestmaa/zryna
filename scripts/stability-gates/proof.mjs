import { compare } from './performance.mjs';
import { reject } from './input.mjs';

export function counts(stdout, proof) {
  const rust = [...stdout.matchAll(/^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;/gm)];
  const tap = [...stdout.matchAll(/^# tests (\d+)\r?\n# suites (\d+)\r?\n# pass (\d+)\r?\n# fail (\d+)\r?\n# cancelled (\d+)\r?\n# skipped (\d+)\r?\n# todo (\d+)$/gm)];
  let passed = 0;
  let failed = 0;
  let skipped = 0;
  if (proof !== 'node-tests') for (const match of rust) {
    passed += Number(match[1]);
    failed += Number(match[2]);
    skipped += Number(match[3]) + Number(match[4]);
  }
  if (proof !== 'rust-tests') for (const match of tap) {
    if (Number(match[1]) !== [3, 4, 5, 6, 7].reduce((sum, index) => sum + Number(match[index]), 0)) {
      reject('inconsistent TAP test inventory');
    }
    passed += Number(match[3]);
    failed += Number(match[4]) + Number(match[5]);
    skipped += Number(match[6]) + Number(match[7]);
  }
  const banners = { m0: /^M0 conformance passed on (?:linux|windows):/m,
    m2: /^M2 conformance passed:/m, m3: /^M3 full gate passed on (?:linux|win32)\/x64\.$/m };
  const complete = (proof === 'node-tests' ? tap.length === 1 :
    proof === 'rust-tests' ? rust.length > 0 : Boolean(banners[proof]?.test(stdout))) &&
    passed > 0 && failed === 0 && skipped === 0 && !/^test result: FAILED\./m.test(stdout);
  return { passed, failed, skipped, complete };
}

export function assessAttempt(attempt, stdout, proof) {
  if (attempt.error === 'ENOENT') return { status: 'blocked', reason: 'required executable unavailable' };
  if (attempt.error !== null || attempt.signal !== null || attempt.exitCode !== 0) {
    return { status: 'failed', reason: 'execution failed or exceeded a resource bound' };
  }
  if (!counts(stdout, proof).complete) return { status: 'failed', reason: 'missing, zero, failed or skipped proof' };
  return { status: 'passed', reason: 'registered current-support proof completed' };
}

export function assessGate(gate, attempts, logs) {
  if (gate.lane !== 'performance') {
    if (attempts.length !== 1) reject('one attempt required for registered gate');
    return assessAttempt(attempts[0], logs[0].stdout, gate.proof);
  }
  if (attempts.length === 0) return { status: 'blocked', reason: 'GNU time memory measurement unavailable' };
  if (attempts.length !== gate.samples + gate.warmups) reject('complete performance attempt inventory required');
  for (const [index, attempt] of attempts.entries()) {
    const assessment = assessAttempt(attempt, logs[index].stdout, gate.proof);
    if (assessment.status !== 'passed') return assessment;
  }
  const samples = attempts.slice(gate.warmups).map((attempt, index) => {
    const markers = [...logs[index + gate.warmups].stderr.matchAll(/^S418-MEMORY-KIB:(\d+)$/gm)];
    if (markers.length !== 1) reject('one GNU time maxrss observation required');
    return { wallMs: attempt.elapsedMs, peakRssKiB: Number(markers[0][1]) };
  });
  return compare(samples, gate.thresholds);
}
