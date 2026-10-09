import {
  maxMatchArmsPerExpression,
  maxMatchArmsPerProject,
  ts,
} from '../boundary/configuration.mjs';
import {
  failBudget,
} from '../boundary/errors.mjs';
import {
  pushExpression,
} from './expression-arena.mjs';
import {
  dataName,
  defensiveNames,
} from './names.mjs';
import {
  nodeSpan,
  spanFromOffsets,
} from './spans.mjs';
import {
  requiredToken,
} from './tokens.mjs';

function normalizeMatchExpression(node, context, depth, normalizeExpression) {
  const { sourceFile, file, collector } = context;
  if (
    !ts.isIdentifier(node.expression) || node.expression.text !== 'match' || node.questionDotToken ||
    node.typeArguments?.length || node.arguments.length !== 2 ||
    !ts.isObjectLiteralExpression(node.arguments[1])
  ) return undefined;
  const scrutinee = normalizeExpression(node.arguments[0], context, depth + 1);
  const object = node.arguments[1];
  if (object.properties.length > maxMatchArmsPerExpression) failBudget('match exceeds the arm limit');
  context.budgets.matchArms += object.properties.length;
  if (context.budgets.matchArms > maxMatchArmsPerProject) failBudget('project exceeds the match-arm limit');
  const arms = [];
  const seen = new Set();
  for (const property of object.properties) {
    if (!ts.isPropertyAssignment(property) || !ts.isStringLiteral(property.name) || !ts.isArrowFunction(property.initializer)) {
      collector.unsupported(property, sourceFile, file, 'match arm');
      return null;
    }
    const raw = property.name.getText(sourceFile);
    const qualified = property.name.text;
    const match = /^([A-Za-z_][A-Za-z0-9_]*)\.([A-Za-z_][A-Za-z0-9_]*)$/.exec(qualified);
    if (
      raw[0] !== '"' || raw.at(-1) !== '"' || raw.slice(1, -1) !== qualified || raw.includes('\\') ||
      !match || defensiveNames.has(match?.[1]) || defensiveNames.has(match?.[2]) || seen.has(qualified)
    ) {
      collector.unsupported(property.name, sourceFile, file, 'match arm key');
      return null;
    }
    seen.add(qualified);
    const arrow = property.initializer;
    const arrowToken = requiredToken(arrow, ts.SyntaxKind.EqualsGreaterThanToken, sourceFile, 'a match arm arrow');
    const parameterSpelling = sourceFile.text.slice(arrow.getStart(sourceFile), arrowToken.getStart(sourceFile)).trim();
    if (
      arrow.modifiers?.length || arrow.typeParameters?.length || arrow.type ||
      arrow.parameters.length > 1 || ts.isBlock(arrow.body) ||
      !parameterSpelling.startsWith('(') || !parameterSpelling.endsWith(')')
    ) {
      collector.unsupported(arrow, sourceFile, file, 'match arm function');
      return null;
    }
    let binding = null;
    if (arrow.parameters.length === 1) {
      const parameter = arrow.parameters[0];
      if (
        !ts.isIdentifier(parameter.name) || parameter.type || parameter.initializer ||
        parameter.questionToken || parameter.dotDotDotToken || parameter.modifiers?.length
      ) {
        collector.unsupported(parameter, sourceFile, file, 'match arm binding');
        return null;
      }
      binding = dataName(parameter.name, sourceFile, file, collector, 'match arm binding');
      if (!binding) return null;
    }
    const value = normalizeExpression(arrow.body, context, depth + 1);
    if (value === null) return null;
    const tokenStart = property.name.getStart(sourceFile) + 1;
    arms.push({
      span: nodeSpan(property, sourceFile, file),
      type_name: {
        text: match[1],
        span: spanFromOffsets(sourceFile, file, tokenStart, tokenStart + match[1].length),
      },
      dot_span: spanFromOffsets(sourceFile, file, tokenStart + match[1].length, tokenStart + match[1].length + 1),
      variant: {
        text: match[2],
        span: spanFromOffsets(sourceFile, file, tokenStart + match[1].length + 1, tokenStart + qualified.length),
      },
      binding, arrow_span: nodeSpan(arrowToken, sourceFile, file), value,
    });
  }
  const openParen = requiredToken(node, ts.SyntaxKind.OpenParenToken, sourceFile, 'a match open parenthesis');
  const closeParen = requiredToken(node, ts.SyntaxKind.CloseParenToken, sourceFile, 'a match close parenthesis');
  const openBrace = requiredToken(object, ts.SyntaxKind.OpenBraceToken, sourceFile, 'a match open brace');
  const closeBrace = requiredToken(object, ts.SyntaxKind.CloseBraceToken, sourceFile, 'a match close brace');
  if (scrutinee === null) return null;
  return pushExpression(context, {
    span: nodeSpan(node, sourceFile, file), kind: {
      kind: 'match', keyword_span: nodeSpan(node.expression, sourceFile, file),
      open_paren_span: nodeSpan(openParen, sourceFile, file), scrutinee,
      close_paren_span: nodeSpan(closeParen, sourceFile, file),
      open_brace_span: nodeSpan(openBrace, sourceFile, file), arms,
      close_brace_span: nodeSpan(closeBrace, sourceFile, file),
    },
  });
}

export {
  normalizeMatchExpression
};
