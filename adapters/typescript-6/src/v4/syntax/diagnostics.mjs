import {
  maxDiagnosticCharacters,
  maxDiagnostics,
  ts,
} from '../boundary/configuration.mjs';
import {
  AdapterError,
} from '../boundary/errors.mjs';
import {
  nodeSpan,
} from './spans.mjs';

function compactText(value) {
  const text = String(value).replaceAll(/\s+/g, ' ').trim();
  return [...text].slice(0, maxDiagnosticCharacters).join('') || 'provider diagnostic';
}

function compareDiagnostics(left, right) {
  const a = left.location.kind === 'source' ? left.location.span : null;
  const b = right.location.kind === 'source' ? right.location.span : null;
  const ak = [a?.file ?? -1, a?.start ?? -1, a?.end ?? -1, left.code, left.message, left.guidance];
  const bk = [b?.file ?? -1, b?.start ?? -1, b?.end ?? -1, right.code, right.message, right.guidance];
  for (let index = 0; index < ak.length; index += 1) {
    if (ak[index] < bk[index]) return -1;
    if (ak[index] > bk[index]) return 1;
  }
  return 0;
}

class DiagnosticCollector {
  #diagnostics = [];
  #truncated = false;

  add(diagnostic) {
    if (this.#diagnostics.length < maxDiagnostics - 1) this.#diagnostics.push(diagnostic);
    else this.#truncated = true;
  }

  located(code, sourceFile, file, node, message, guidance) {
    this.add({
      code,
      severity: 'error',
      location: { kind: 'source', span: nodeSpan(node, sourceFile, file) },
      message: compactText(message),
      guidance: compactText(guidance),
    });
  }

  unsupported(node, sourceFile, file, context) {
    const kind = ts.SyntaxKind[node.kind] ?? `kind ${node.kind}`;
    const span = nodeSpan(node, sourceFile, file);
    throw new AdapterError(
      'ZRYNA-F2002',
      `${context} uses unsupported syntax '${kind}' at file ${span.file} bytes ${span.start}..${span.end}`,
    );
  }

  finish() {
    this.#diagnostics.sort(compareDiagnostics);
    if (this.#truncated) {
      this.#diagnostics.push({
        code: 'ZRYNA-F2003', severity: 'error', location: { kind: 'global' },
        message: 'frontend diagnostics exceeded the deterministic limit',
        guidance: 'reduce unsupported or malformed source before analysis',
      });
    }
    return this.#diagnostics;
  }
}

export {
  DiagnosticCollector,
  compactText
};
