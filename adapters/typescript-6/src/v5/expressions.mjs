// Isolated protocol-v5 expressions; inherited v4 forms retain their source-only meaning.
import ts from '@typescript/typescript6';
import {
  maxNesting,
  maxCallArguments,
  maxIntegerSpellingBytes,
  failBudget,
  failInvariant,
  nodeSpan,
  normalizedIdentifier,
  requiredToken,
  dataName
} from './common.mjs';
import {
  normalizeTypeArguments
} from './types.mjs';
import {
  pushExpression,
  countAggregateOperands,
  normalizeConstructionFields,
  normalizeArrayConstruction,
  normalizeMatchExpression
} from './construction.mjs';

const binaryKinds = new Map([
  [ts.SyntaxKind.PlusToken, 'addition'],
  [ts.SyntaxKind.MinusToken, 'subtraction'],
  [ts.SyntaxKind.AsteriskToken, 'multiplication'],
  [ts.SyntaxKind.EqualsEqualsEqualsToken, 'equal'],
  [ts.SyntaxKind.ExclamationEqualsEqualsToken, 'not-equal'],
  [ts.SyntaxKind.LessThanToken, 'less-than'],
  [ts.SyntaxKind.LessThanEqualsToken, 'less-equal'],
  [ts.SyntaxKind.GreaterThanToken, 'greater-than'],
  [ts.SyntaxKind.GreaterThanEqualsToken, 'greater-equal'],
]);

export function normalizeExpression(node, context, depth = 1) {
  const { sourceFile, file, collector } = context;
  if (depth > maxNesting) {
    collector.unsupported(node, sourceFile, file, 'expression depth');
    return null;
  }
  if (ts.isIdentifier(node)) {
    const name = normalizedIdentifier(node, sourceFile, file, collector, 'reference');
    return name === null ? null : pushExpression(context, { span: nodeSpan(node, sourceFile, file), kind: { kind: 'reference', name } });
  }
  if (node.kind === ts.SyntaxKind.TrueKeyword || node.kind === ts.SyntaxKind.FalseKeyword) {
    return pushExpression(context, { span: nodeSpan(node, sourceFile, file), kind: { kind: 'bool-literal', value: node.kind === ts.SyntaxKind.TrueKeyword } });
  }
  if (ts.isStringLiteral(node)) {
    const spelling = node.getText(sourceFile);
    if ((spelling[0] !== '"' && spelling[0] !== "'") || spelling.at(-1) !== spelling[0] || spelling.includes('\\')) {
      collector.unsupported(node, sourceFile, file, 'string literal');
      return null;
    }
    return pushExpression(context, {
      span: nodeSpan(node, sourceFile, file), kind: { kind: 'string-literal', spelling },
    });
  }
  if (ts.isNumericLiteral(node)) {
    const spelling = node.getText(sourceFile);
    if (Buffer.byteLength(spelling, 'utf8') <= maxIntegerSpellingBytes && /^(0|[1-9][0-9]*)$/.test(spelling)) {
      return pushExpression(context, { span: nodeSpan(node, sourceFile, file), kind: { kind: 'i32-literal', spelling } });
    }
  } else if (
    ts.isPrefixUnaryExpression(node) &&
    node.operator === ts.SyntaxKind.MinusToken &&
    ts.isNumericLiteral(node.operand) &&
    Buffer.byteLength(node.getText(sourceFile), 'utf8') <= maxIntegerSpellingBytes &&
    /^-[1-9][0-9]*$/.test(node.getText(sourceFile))
  ) {
    return pushExpression(context, {
      span: nodeSpan(node, sourceFile, file),
      kind: { kind: 'i32-literal', spelling: node.getText(sourceFile) },
    });
  } else if (ts.isPrefixUnaryExpression(node) && node.operator === ts.SyntaxKind.MinusToken) {
    const operand = normalizeExpression(node.operand, context, depth + 1);
    if (operand !== null) {
      const operator = node.getFirstToken(sourceFile);
      if (!operator || operator.kind !== ts.SyntaxKind.MinusToken) failInvariant('TypeScript omitted a unary minus token');
      return pushExpression(context, { span: nodeSpan(node, sourceFile, file), kind: { kind: 'negation', operator_span: nodeSpan(operator, sourceFile, file), operand } });
    }
    return null;
  } else if (ts.isBinaryExpression(node) && binaryKinds.has(node.operatorToken.kind)) {
    const lhs = normalizeExpression(node.left, context, depth + 1);
    const rhs = normalizeExpression(node.right, context, depth + 1);
    if (lhs !== null && rhs !== null) {
      return pushExpression(context, { span: nodeSpan(node, sourceFile, file), kind: { kind: binaryKinds.get(node.operatorToken.kind), operator_span: nodeSpan(node.operatorToken, sourceFile, file), lhs, rhs } });
    }
    return null;
  } else if (ts.isPropertyAccessExpression(node) && !node.questionDotToken && ts.isIdentifier(node.name)) {
    const base = normalizeExpression(node.expression, context, depth + 1);
    const field = dataName(node.name, sourceFile, file, collector, 'field access name');
    const dot = requiredToken(node, ts.SyntaxKind.DotToken, sourceFile, 'a field-access dot');
    if (base !== null && field) {
      return pushExpression(context, {
        span: nodeSpan(node, sourceFile, file),
        kind: { kind: 'field-access', base, dot_span: nodeSpan(dot, sourceFile, file), field },
      });
    }
    return null;
  } else if (ts.isElementAccessExpression(node) && node.argumentExpression && !node.questionDotToken) {
    const base = normalizeExpression(node.expression, context, depth + 1);
    const index = normalizeExpression(node.argumentExpression, context, depth + 1);
    const open = requiredToken(node, ts.SyntaxKind.OpenBracketToken, sourceFile, 'an index open bracket');
    const close = requiredToken(node, ts.SyntaxKind.CloseBracketToken, sourceFile, 'an index close bracket');
    if (base !== null && index !== null) {
      return pushExpression(context, {
        span: nodeSpan(node, sourceFile, file),
        kind: {
          kind: 'index', base, open_bracket_span: nodeSpan(open, sourceFile, file), index,
          close_bracket_span: nodeSpan(close, sourceFile, file),
        },
      });
    }
    return null;
  } else if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === 'match') {
    const normalized = normalizeMatchExpression(node, context, depth);
    if (normalized !== undefined) return normalized;
    collector.unsupported(node, sourceFile, file, 'match expression');
    return null;
  } else if (ts.isCallExpression(node) && node.typeArguments?.length) {
    const normalized = normalizeArrayConstruction(node, context, depth);
    if (normalized !== undefined) return normalized;
  }
  if (
    ts.isCallExpression(node) && ts.isIdentifier(node.expression) && !node.questionDotToken &&
    node.arguments.length === 1 && ts.isObjectLiteralExpression(node.arguments[0]) &&
    !new Set(['clone', 'borrow', 'borrowMut', 'shared', 'downgrade', 'push', 'match', 'upgradeWeak']).has(node.expression.text)
  ) {
    const typeArguments = normalizeTypeArguments(node, sourceFile, file, collector, context.typeSyntax, context.budgets);
    const typeName = dataName(node.expression, sourceFile, file, collector, 'struct construction type');
    const fields = normalizeConstructionFields(node.arguments[0], context, depth);
    const openParen = requiredToken(node, ts.SyntaxKind.OpenParenToken, sourceFile, 'a construction open parenthesis');
    const closeParen = requiredToken(node, ts.SyntaxKind.CloseParenToken, sourceFile, 'a construction close parenthesis');
    const openBrace = requiredToken(node.arguments[0], ts.SyntaxKind.OpenBraceToken, sourceFile, 'a construction open brace');
    const closeBrace = requiredToken(node.arguments[0], ts.SyntaxKind.CloseBraceToken, sourceFile, 'a construction close brace');
    if (typeName && fields) {
      return pushExpression(context, {
        span: nodeSpan(node, sourceFile, file), kind: {
          kind: 'struct-construction', type_name: typeName, type_arguments: typeArguments,
          open_paren_span: nodeSpan(openParen, sourceFile, file),
          open_brace_span: nodeSpan(openBrace, sourceFile, file), fields,
          close_brace_span: nodeSpan(closeBrace, sourceFile, file),
          close_paren_span: nodeSpan(closeParen, sourceFile, file),
        },
      });
    }
    return null;
  } else if (
    ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression) &&
    ts.isIdentifier(node.expression.expression) && ts.isIdentifier(node.expression.name) &&
    !node.questionDotToken && !node.expression.questionDotToken && node.arguments.length <= 1
  ) {
    const typeArguments = normalizeTypeArguments(node, sourceFile, file, collector, context.typeSyntax, context.budgets);
    const typeName = dataName(node.expression.expression, sourceFile, file, collector, 'enum construction type');
    const variant = dataName(node.expression.name, sourceFile, file, collector, 'enum construction variant');
    const dot = requiredToken(node.expression, ts.SyntaxKind.DotToken, sourceFile, 'an enum construction dot');
    const open = requiredToken(node, ts.SyntaxKind.OpenParenToken, sourceFile, 'an enum construction open parenthesis');
    const close = requiredToken(node, ts.SyntaxKind.CloseParenToken, sourceFile, 'an enum construction close parenthesis');
    const payload = node.arguments.length === 1 ? normalizeExpression(node.arguments[0], context, depth + 1) : null;
    countAggregateOperands(context, node.arguments.length);
    if (typeName && variant && (node.arguments.length === 0 || payload !== null)) {
      return pushExpression(context, {
        span: nodeSpan(node, sourceFile, file), kind: {
          kind: 'enum-construction', type_name: typeName, dot_span: nodeSpan(dot, sourceFile, file),
          variant, type_arguments: typeArguments, open_paren_span: nodeSpan(open, sourceFile, file), payload,
          close_paren_span: nodeSpan(close, sourceFile, file),
        },
      });
    }
    return null;
  } else if (
    ts.isCallExpression(node) && ts.isIdentifier(node.expression) && !node.typeArguments?.length &&
    !node.questionDotToken && ['clone', 'borrow', 'borrowMut', 'shared', 'downgrade'].includes(node.expression.text)
  ) {
    if (node.arguments.length !== 1 || ts.isSpreadElement(node.arguments[0])) {
      collector.unsupported(node, sourceFile, file, 'ownership intrinsic');
      return null;
    }
    const value = normalizeExpression(node.arguments[0], context, depth + 1);
    const open = requiredToken(node, ts.SyntaxKind.OpenParenToken, sourceFile, 'an intrinsic open parenthesis');
    const close = requiredToken(node, ts.SyntaxKind.CloseParenToken, sourceFile, 'an intrinsic close parenthesis');
    const kind = new Map([
      ['clone', 'clone'], ['borrow', 'borrow'], ['borrowMut', 'borrow-mut'],
      ['shared', 'shared'], ['downgrade', 'downgrade'],
    ]).get(node.expression.text);
    if (value !== null) return pushExpression(context, {
      span: nodeSpan(node, sourceFile, file), kind: {
        kind, keyword_span: nodeSpan(node.expression, sourceFile, file),
        open_paren_span: nodeSpan(open, sourceFile, file), value,
        close_paren_span: nodeSpan(close, sourceFile, file),
      },
    });
    return null;
  } else if (
    ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === 'push' &&
    !node.typeArguments?.length && !node.questionDotToken
  ) {
    if (node.arguments.length !== 2 || node.arguments.some(ts.isSpreadElement)) {
      collector.unsupported(node, sourceFile, file, 'vector push');
      return null;
    }
    const vector = normalizeExpression(node.arguments[0], context, depth + 1);
    const value = normalizeExpression(node.arguments[1], context, depth + 1);
    const open = requiredToken(node, ts.SyntaxKind.OpenParenToken, sourceFile, 'a vector-push open parenthesis');
    const comma = requiredToken(node, ts.SyntaxKind.CommaToken, sourceFile, 'a vector-push comma');
    const close = requiredToken(node, ts.SyntaxKind.CloseParenToken, sourceFile, 'a vector-push close parenthesis');
    if (vector !== null && value !== null) return pushExpression(context, {
      span: nodeSpan(node, sourceFile, file), kind: {
        kind: 'vec-push', keyword_span: nodeSpan(node.expression, sourceFile, file),
        open_paren_span: nodeSpan(open, sourceFile, file), vector,
        comma_span: nodeSpan(comma, sourceFile, file), value,
        close_paren_span: nodeSpan(close, sourceFile, file),
      },
    });
    return null;
  } else if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && !node.questionDotToken) {
    if (node.arguments.length > maxCallArguments) failBudget('call exceeds the argument limit');
    const typeArguments = normalizeTypeArguments(node, sourceFile, file, collector, context.typeSyntax, context.budgets);
    const callee = normalizedIdentifier(node.expression, sourceFile, file, collector, 'call callee');
    const args = [];
    let valid = callee !== null;
    for (const argument of node.arguments) {
      if (ts.isSpreadElement(argument)) {
        collector.unsupported(argument, sourceFile, file, 'call argument');
        valid = false;
        continue;
      }
      const normalized = normalizeExpression(argument, context, depth + 1);
      if (normalized === null) valid = false;
      else args.push(normalized);
    }
    const open = requiredToken(node, ts.SyntaxKind.OpenParenToken, sourceFile, 'a call open parenthesis');
    const close = requiredToken(node, ts.SyntaxKind.CloseParenToken, sourceFile, 'a call close parenthesis');
    if (valid) return pushExpression(context, { span: nodeSpan(node, sourceFile, file), kind: { kind: 'call', callee, type_arguments: typeArguments, open_paren_span: nodeSpan(open, sourceFile, file), arguments: args, close_paren_span: nodeSpan(close, sourceFile, file) } });
    return null;
  }
  collector.unsupported(node, sourceFile, file, 'expression');
  return null;
}

