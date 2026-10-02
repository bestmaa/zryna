// Validates the unchanged compiler-owned structured-v1 report as inert presentation data.
import { exact, fail, limits, sourcePath, utf8 } from './limits.mjs';

export function verifyReport(report, source) {
  exact(report, ['schema_version', 'diagnostics']);
  if (report.schema_version !== 1 || !Array.isArray(report.diagnostics) ||
      report.diagnostics.length > limits.diagnostics ||
      utf8(JSON.stringify(report)).length > limits.report) fail('REPORT');
  const sourceBytes = utf8(source);
  const starts = [0];
  for (let index = 0; index < sourceBytes.length; index++) {
    if (sourceBytes[index] === 13 && sourceBytes[index + 1] === 10) index++;
    else if (![10, 13].includes(sourceBytes[index])) continue;
    starts.push(index + 1);
  }
  function position(offset) {
    const line = starts.findLastIndex(start => start <= offset);
    const prefix = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true })
      .decode(sourceBytes.subarray(starts[line], offset));
    return [line + 1, Array.from(prefix).length + 1];
  }
  let previous;
  for (const record of report.diagnostics) {
    exact(record, ['code', 'severity', 'path', 'byte_start', 'byte_end', 'line_start',
      'column_start', 'line_end', 'column_end', 'message', 'guidance']);
    if (typeof record.code !== 'string' || !/^(?:ZRYNA-[A-Z][0-9]{4}|TS[0-9]{1,10})$/.test(record.code) ||
        !['error', 'warning'].includes(record.severity) ||
        utf8(record.message).length > limits.text || utf8(record.guidance).length > limits.text ||
        !(record.path === null || record.path === sourcePath)) fail('REPORT');
    if (previous && compare(previous, record) > 0) fail('REPORT-ORDER');
    previous = record;
    const coordinates = ['byte_start', 'byte_end', 'line_start', 'column_start', 'line_end', 'column_end'];
    if (record.byte_start === null) {
      if (coordinates.some(key => record[key] !== null)) fail('REPORT-SPAN');
      continue;
    }
    if (record.path !== sourcePath || coordinates.some(key => !Number.isSafeInteger(record[key])) ||
        record.byte_start < 0 || record.byte_end < record.byte_start ||
        record.byte_end > sourceBytes.length ||
        coordinates.slice(2).some(key => record[key] < 1)) fail('REPORT-SPAN');
    try {
      const decoder = new TextDecoder('utf-8', { fatal: true });
      decoder.decode(sourceBytes.subarray(0, record.byte_start));
      decoder.decode(sourceBytes.subarray(0, record.byte_end));
      const start = position(record.byte_start);
      const end = position(record.byte_end);
      if (start[0] !== record.line_start || start[1] !== record.column_start ||
          end[0] !== record.line_end || end[1] !== record.column_end) fail('REPORT-SPAN');
    } catch { fail('REPORT-SPAN'); }
  }
}

// Rust's Option and UTF-8 lexical ordering, including non-BMP diagnostic text.
function compare(left, right) {
  for (const key of ['path', 'byte_start', 'byte_end', 'severity', 'code', 'message', 'guidance']) {
    const a = left[key]; const b = right[key];
    if (a === b) continue;
    if (a === null || b === null) return a === null ? -1 : 1;
    if (typeof a === 'number') return a < b ? -1 : 1;
    const bytesA = utf8(a); const bytesB = utf8(b);
    for (let index = 0; index < Math.min(bytesA.length, bytesB.length); index++) {
      if (bytesA[index] !== bytesB[index]) return bytesA[index] < bytesB[index] ? -1 : 1;
    }
    return bytesA.length < bytesB.length ? -1 : 1;
  }
  return 0;
}
