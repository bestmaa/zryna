import {
  maxNesting,
  ts,
} from '../boundary/configuration.mjs';
import {
  failBudget,
  failInvariant,
} from '../boundary/errors.mjs';

function findToken(node, kind, sourceFile) {
  const children = node.getChildren(sourceFile);
  const direct = children.find((child) => child.kind === kind);
  if (direct) return direct;
  for (const child of children) {
    const nested = findToken(child, kind, sourceFile);
    if (nested) return nested;
  }
  return null;
}

function requiredToken(node, kind, sourceFile, label) {
  const token = findToken(node, kind, sourceFile);
  if (!token) failInvariant(`TypeScript omitted ${label}`);
  return token;
}

function enforceParserNesting(text) {
  const scanner = ts.createScanner(ts.ScriptTarget.Latest, true, ts.LanguageVariant.Standard, text);
  let depth = 0;
  for (let token = scanner.scan(); token !== ts.SyntaxKind.EndOfFileToken; token = scanner.scan()) {
    if (token === ts.SyntaxKind.OpenBraceToken || token === ts.SyntaxKind.OpenBracketToken || token === ts.SyntaxKind.OpenParenToken) {
      depth += 1;
      if (depth > maxNesting) failBudget('source exceeds the nesting limit');
    } else if (token === ts.SyntaxKind.CloseBraceToken || token === ts.SyntaxKind.CloseBracketToken || token === ts.SyntaxKind.CloseParenToken) {
      depth = Math.max(0, depth - 1);
    }
  }
}

export {
  enforceParserNesting,
  findToken,
  requiredToken
};
