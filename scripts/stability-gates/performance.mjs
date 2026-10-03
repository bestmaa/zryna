import { exact, reject } from './input.mjs';

export function summarize(samples) {
  if (!Array.isArray(samples) || samples.length !== 5) reject('five measured samples required');
  for (const sample of samples) {
    exact(sample, ['wallMs', 'peakRssKiB'], 'performance sample');
    for (const value of Object.values(sample)) {
      if (!Number.isFinite(value) || value <= 0) reject('positive finite performance measurement required');
    }
  }
  const metric = key => {
    const values = samples.map(sample => sample[key]).sort((a, b) => a - b);
    return { median: values[2], maximum: values[4], spread: (values[4] - values[0]) / values[2] };
  };
  return { wallMs: metric('wallMs'), peakRssKiB: metric('peakRssKiB') };
}

// Registry v1 deliberately supplies no threshold authority. Review must freeze actual host,
// toolchain, workload, method and independent baseline before a future registry can use this.
export function compare(samples, thresholds) {
  const summary = summarize(samples);
  if (thresholds === null) return { status: 'blocked', reason: 'reviewed performance baseline absent', summary };
  exact(thresholds, ['wallMs', 'peakRssKiB', 'maxSpread'], 'thresholds');
  for (const value of Object.values(thresholds)) {
    if (!Number.isFinite(value) || value <= 0) reject('positive finite thresholds required');
  }
  const failures = [];
  for (const key of ['wallMs', 'peakRssKiB']) {
    if (summary[key].maximum > thresholds[key]) failures.push(`${key} exceeds ceiling`);
    if (summary[key].spread > thresholds.maxSpread) failures.push(`${key} exceeds variance policy`);
  }
  return { status: failures.length ? 'failed' : 'passed', failures, summary };
}
