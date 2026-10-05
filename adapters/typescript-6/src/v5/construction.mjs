// Isolated protocol-v5 construction; inherited v4 forms retain their source-only meaning.
import ts from '@typescript/typescript6';
import {
  maxExpressionsPerFunction,
  maxExpressionsPerProject,
  maxObjectInitializersPerConstruction,
  maxArrayElementsPerConstruction,
  maxConstructionOperandsPerProject,
  maxMatchArmsPerExpression,
  maxMatchArmsPerProject,
  maxFixedArrayLength,
  maxFixedArrayLengthSpellingBytes,
  failBudget,
  spanFromOffsets,
  nodeSpan,
  requiredToken,
  defensiveNames,
  dataName
} from './common.mjs';
import {
  genericList,
  pushType,
  normalizeType
} from './types.mjs';
import {
  normalizeExpression
} from './expressions.mjs';

export function pushExpression(context, expression) {
  if (context.expressions.length >= maxExpressionsPerFunction) failBudget('function exceeds the expression limit');
  context.budgets.expressions += 1;
  if (context.budgets.expressions > maxExpressionsPerProject) failBudget('project exceeds the expression limit');
  const id = context.expressions.length;
  context.expressions.push(expression);
  return id;
}

export function countAggregateOperands(context, count) {
  context.budgets.aggregateOperands += count;
  if (context.budgets.aggregateOperands > maxConstructionOperandsPerProject) {
    failBudget('project exceeds the aggregate-construction operand limit');
  }
}

export function normalizeConstructionFields(object, context, depth) {
  const { sourceFile, file, collector } = context;
  const fields = [];
  const seen = new Set();
  if (object.properties.length > maxObjectInitializersPerConstruction) {
    failBudget('struct construction exceeds the initializer limit');
  }
  countAggregateOperands(context, object.properties.length);
  for (const property of object.properties) {
    if (!ts.isPropertyAssignment(property) && !ts.isShorthandPropertyAssignment(property)) {
      collector.unsupported(property, sourceFile, file, 'aggregate construction field');
      return null;
    }
    if (!ts.isIdentifier(property.name) || property.modifiers?.length) {
      collector.unsupported(property, sourceFile, file, 'aggregate construction field name');
      return null;
    }
    const name = dataName(property.name, sourceFile, file, collector, 'aggregate construction field name');
    if (!name || seen.has(name.text)) {
      collector.unsupported(property.name, sourceFile, file, 'duplicate aggregate construction field');
      return null;
    }
    seen.add(name.text);
    if (ts.isShorthandPropertyAssignment(property)) {
      if (property.objectAssignmentInitializer) {
        collector.unsupported(property, sourceFile, file, 'aggregate shorthand initializer');
        return null;
      }
      const value = pushExpression(context, {
        span: nodeSpan(property.name, sourceFile, file), kind: { kind: 'reference', name },
      });
      fields.push({
        span: nodeSpan(property, sourceFile, file),
        kind: { kind: 'shorthand', name, value },
      });
      continue;
    }
    const colon = requiredToken(property, ts.SyntaxKind.ColonToken, sourceFile, 'an aggregate field colon');
    const value = normalizeExpression(property.initializer, context, depth + 1);
    if (value === null) return null;
    fields.push({
      span: nodeSpan(property, sourceFile, file),
      kind: { kind: 'explicit', name, colon_span: nodeSpan(colon, sourceFile, file), value },
    });
  }
  return fields;
}

export function normalizeArrayConstruction(node, context, depth) {
  const { sourceFile, file, collector, typeSyntax, budgets } = context;
  if (
    !ts.isIdentifier(node.expression) || node.questionDotToken ||
    !node.typeArguments || node.arguments.length !== 1 ||
    !ts.isArrayLiteralExpression(node.arguments[0])
  ) return undefined;
  const spelling = node.expression.text;
  if (spelling !== 'Vec' && spelling !== 'FixedArray') return undefined;
  const requiredArguments = spelling === 'Vec' ? 1 : 2;
  if (node.typeArguments.length !== requiredArguments) {
    collector.unsupported(node, sourceFile, file, 'typed array construction');
    return null;
  }
  const array = node.arguments[0];
  if (array.elements.length > maxArrayElementsPerConstruction) {
    failBudget('array construction exceeds the element limit');
  }
  const name = dataName(node.expression, sourceFile, file, collector, 'array construction type');
  if (!name) return null;
  const less = requiredToken(node, ts.SyntaxKind.LessThanToken, sourceFile, 'a type-argument open token');
  const greater = requiredToken(node, ts.SyntaxKind.GreaterThanToken, sourceFile, 'a type-argument close token');
  const element = normalizeType(
    node.typeArguments[0], node.typeArguments[0].getStart(sourceFile), sourceFile, file, collector,
    'array element type', typeSyntax, budgets,
  );
  if (element === null) return null;
  let typeKind;
  if (spelling === 'Vec') {
    typeKind = {
      kind: 'vec', keyword_span: name.span, less_than_span: nodeSpan(less, sourceFile, file),
      argument: element, greater_than_span: nodeSpan(greater, sourceFile, file),
    };
  } else {
    const lengthNode = node.typeArguments[1];
    if (!ts.isLiteralTypeNode(lengthNode) || !ts.isNumericLiteral(lengthNode.literal)) {
      collector.unsupported(lengthNode, sourceFile, file, 'fixed-array length');
      return null;
    }
    const lengthSpelling = lengthNode.literal.getText(sourceFile);
    if (
      !/^(0|[1-9][0-9]*)$/.test(lengthSpelling) ||
      Buffer.byteLength(lengthSpelling, 'utf8') > maxFixedArrayLengthSpellingBytes ||
      BigInt(lengthSpelling) > BigInt(maxFixedArrayLength)
    ) {
      failBudget(`fixed-array length must be canonical and at most ${maxFixedArrayLength}`);
    }
    const commaSpan = genericList(node, node.typeArguments, sourceFile, file, collector, {}).comma_spans[0];
    typeKind = {
      kind: 'fixed-array', keyword_span: name.span, less_than_span: nodeSpan(less, sourceFile, file),
      element, comma_span: commaSpan,
      length_span: nodeSpan(lengthNode.literal, sourceFile, file),
      length: Number(lengthSpelling), length_spelling: lengthSpelling,
      greater_than_span: nodeSpan(greater, sourceFile, file),
    };
  }
  const typeId = pushType(typeSyntax, budgets, {
    span: spanFromOffsets(sourceFile, file, node.expression.getStart(sourceFile), greater.getEnd()),
    kind: typeKind,
  });
  const elements = [];
  for (const element of array.elements) {
    if (ts.isOmittedExpression(element) || ts.isSpreadElement(element)) {
      collector.unsupported(element, sourceFile, file, 'array construction element');
      return null;
    }
    const value = normalizeExpression(element, context, depth + 1);
    if (value === null) return null;
    elements.push(value);
  }
  countAggregateOperands(context, elements.length);
  const openParen = requiredToken(node, ts.SyntaxKind.OpenParenToken, sourceFile, 'an array construction open parenthesis');
  const closeParen = requiredToken(node, ts.SyntaxKind.CloseParenToken, sourceFile, 'an array construction close parenthesis');
  const openBracket = requiredToken(array, ts.SyntaxKind.OpenBracketToken, sourceFile, 'an array construction open bracket');
  const closeBracket = requiredToken(array, ts.SyntaxKind.CloseBracketToken, sourceFile, 'an array construction close bracket');
  return pushExpression(context, {
    span: nodeSpan(node, sourceFile, file), kind: {
      kind: spelling === 'Vec' ? 'vec-construction' : 'fixed-array-construction',
      type_syntax: typeId, open_paren_span: nodeSpan(openParen, sourceFile, file),
      open_bracket_span: nodeSpan(openBracket, sourceFile, file), elements,
      close_bracket_span: nodeSpan(closeBracket, sourceFile, file),
      close_paren_span: nodeSpan(closeParen, sourceFile, file),
    },
  });
}

export function normalizeMatchExpression(node, context, depth) {
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
