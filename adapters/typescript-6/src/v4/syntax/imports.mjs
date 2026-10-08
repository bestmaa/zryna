import {
  maxBindingsPerImport,
  maxBindingsPerProject,
  ts,
} from '../boundary/configuration.mjs';
import {
  failBudget,
} from '../boundary/errors.mjs';
import {
  normalizedIdentifier,
} from './names.mjs';
import {
  nodeSpan,
  spanFromOffsets,
} from './spans.mjs';
import {
  findToken,
  requiredToken,
} from './tokens.mjs';

function normalizeImport(node, sourceFile, file, collector, budgets) {
  let valid = true;
  if (!node.importClause || node.importClause.isTypeOnly || node.importClause.name || !node.importClause.namedBindings || !ts.isNamedImports(node.importClause.namedBindings) || node.attributes) {
    collector.unsupported(node, sourceFile, file, 'import declaration');
    return null;
  }
  if (!ts.isStringLiteral(node.moduleSpecifier)) {
    collector.unsupported(node.moduleSpecifier, sourceFile, file, 'module specifier');
    return null;
  }
  const elements = node.importClause.namedBindings.elements;
  if (elements.length === 0) {
    collector.unsupported(node.importClause.namedBindings, sourceFile, file, 'empty named import');
    valid = false;
  }
  if (elements.length > maxBindingsPerImport) failBudget('import exceeds the imported-name limit');
  budgets.bindings += elements.length;
  if (budgets.bindings > maxBindingsPerProject) failBudget('project exceeds the imported-name limit');
  const raw = node.moduleSpecifier.getText(sourceFile);
  const quote = raw[0];
  const value = node.moduleSpecifier.text;
  const components = value.split('/');
  const specifierBody = value.startsWith('./')
    ? value.slice(2)
    : value.replace(/^(?:\.\.\/)+/, '');
  const validSpecifier =
    (value.startsWith('./') || value.startsWith('../')) &&
    value.endsWith('.zry') &&
    /^[\x00-\x7f]*$/.test(value) &&
    !value.includes('\\') &&
    !value.includes('\0') &&
    !value.includes('?') &&
    !value.includes('#') &&
    !value.includes('://') &&
    specifierBody.length > 0 &&
    components.every((component) => component.length > 0);
  if ((quote !== '"' && quote !== "'") || raw.at(-1) !== quote || raw.slice(1, -1) !== value || raw.includes('\\') || Buffer.byteLength(value, 'utf8') > 1024 || !validSpecifier) {
    collector.unsupported(node.moduleSpecifier, sourceFile, file, 'module specifier');
    valid = false;
  }
  const bindings = [];
  for (const element of elements) {
    if (element.isTypeOnly) {
      collector.unsupported(element, sourceFile, file, 'type-only import');
      valid = false;
      continue;
    }
    const importedNode = element.propertyName ?? element.name;
    const imported = normalizedIdentifier(importedNode, sourceFile, file, collector, 'imported name');
    const local = normalizedIdentifier(element.name, sourceFile, file, collector, 'import local name');
    const asToken = element.propertyName ? requiredToken(element, ts.SyntaxKind.AsKeyword, sourceFile, 'an import alias keyword') : null;
    if (!imported || !local) valid = false;
    else bindings.push({ span: nodeSpan(element, sourceFile, file), imported, local, as_span: asToken ? nodeSpan(asToken, sourceFile, file) : null });
  }
  const importToken = requiredToken(node, ts.SyntaxKind.ImportKeyword, sourceFile, 'an import keyword');
  const fromToken = requiredToken(node, ts.SyntaxKind.FromKeyword, sourceFile, 'an import from keyword');
  const semicolon = findToken(node, ts.SyntaxKind.SemicolonToken, sourceFile);
  if (!semicolon) {
    collector.unsupported(node, sourceFile, file, 'import without a semicolon');
    valid = false;
  }
  if (!valid || !semicolon) return null;
  const tokenStart = node.moduleSpecifier.getStart(sourceFile);
  return {
    span: nodeSpan(node, sourceFile, file),
    import_span: nodeSpan(importToken, sourceFile, file),
    bindings,
    from_span: nodeSpan(fromToken, sourceFile, file),
    specifier: {
      text: value,
      token_span: nodeSpan(node.moduleSpecifier, sourceFile, file),
      value_span: spanFromOffsets(sourceFile, file, tokenStart + 1, node.moduleSpecifier.getEnd() - 1),
    },
    semicolon_span: nodeSpan(semicolon, sourceFile, file),
  };
}

export {
  normalizeImport
};
