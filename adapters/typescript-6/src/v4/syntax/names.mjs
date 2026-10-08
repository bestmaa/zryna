import {
  maxIdentifierBytes,
} from '../boundary/configuration.mjs';
import {
  nodeSpan,
} from './spans.mjs';

function normalizedIdentifier(node, sourceFile, file, collector, context) {
  const spelling = node.getText(sourceFile);
  if (
    spelling !== node.text || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(spelling) ||
    Buffer.byteLength(spelling, 'utf8') > maxIdentifierBytes || defensiveNames.has(spelling)
  ) {
    collector.unsupported(node, sourceFile, file, context);
    return null;
  }
  return { text: spelling, span: nodeSpan(node, sourceFile, file) };
}

const defensiveNames = new Set(['constructor', 'prototype', '__proto__']);

function dataName(node, sourceFile, file, collector, context) {
  const name = normalizedIdentifier(node, sourceFile, file, collector, context);
  if (!name) return null;
  if (Buffer.byteLength(name.text, 'utf8') > maxIdentifierBytes || defensiveNames.has(name.text)) {
    collector.unsupported(node, sourceFile, file, context);
    return null;
  }
  return name;
}

export {
  dataName,
  defensiveNames,
  normalizedIdentifier
};
