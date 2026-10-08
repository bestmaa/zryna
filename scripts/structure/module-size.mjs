import { createRequire } from 'node:module';
import { fail, physicalLines } from './policy.mjs';

const requireAdapter = createRequire(new URL('../../adapters/typescript-6/package.json', import.meta.url));
let typescript;

// Read-only normalization: input bytes, comments and blank lines always count in full.
// The pinned AST printer additionally expands compressed JS/TS statements and blocks.
export function moduleLines(path, text) {
  const physical = physicalLines(text);
  if (!/\.(?:mjs|cjs|js|ts|jsx|tsx|mts|cts)$/i.test(path)) return physical;
  try {
    typescript ??= requireAdapter('@typescript/typescript6');
  } catch {
    fail('module-size normalization requires the frozen adapter dependency; run pnpm install --frozen-lockfile');
  }
  if (typescript.version !== '6.0.3') fail(`module-size printer version ${typescript.version}; expected pinned 6.0.3`);
  const kind = /\.tsx$/i.test(path) ? typescript.ScriptKind.TSX : /\.jsx$/i.test(path)
    ? typescript.ScriptKind.JSX : /\.(?:ts|mts|cts)$/i.test(path) ? typescript.ScriptKind.TS : typescript.ScriptKind.JS;
  const file = typescript.createSourceFile(path, text, typescript.ScriptTarget.Latest, true, kind);
  if (file.parseDiagnostics.length) {
    fail(`cannot normalize ${path}: ${typescript.flattenDiagnosticMessageText(file.parseDiagnostics[0].messageText, ' ')}`);
  }
  const printer = typescript.createPrinter({ newLine: typescript.NewLineKind.LineFeed, removeComments: false });
  let meaningful = 0;
  function visit(node) {
    if ((typescript.isStatement(node) && !typescript.isBlock(node) && !typescript.isVariableStatement(node))
      || (typescript.isVariableDeclaration(node) && typescript.isIdentifier(node.name))
      || (typescript.isBindingElement(node) && typescript.isIdentifier(node.name))
      || (typescript.isBinaryExpression(node) && node.operatorToken.kind === typescript.SyntaxKind.CommaToken)) {
      meaningful += 1;
    }
    typescript.forEachChild(node, visit);
  }
  visit(file);
  return Math.max(physical, physicalLines(printer.printFile(file)), meaningful);
}
