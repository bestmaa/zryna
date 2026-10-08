import {
  maxArrayElementsPerConstruction,
  maxFixedArrayLength,
  maxFixedArrayLengthSpellingBytes,
  maxObjectInitializersPerConstruction,
  ts,
} from '../boundary/configuration.mjs';
import {
  failBudget,
} from '../boundary/errors.mjs';
import {
  countAggregateOperands,
  pushExpression,
} from './expression-arena.mjs';
import {
  dataName,
} from './names.mjs';
import {
  nodeSpan,
  spanFromOffsets,
} from './spans.mjs';
import {
  requiredToken,
} from './tokens.mjs';
import {
  normalizeType,
  pushType,
} from './types.mjs';

function normalizeConstructionFields(object, context, depth, normalizeExpression) {
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

function normalizeArrayConstruction(node, context, depth, normalizeExpression) {
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
    const comma = requiredToken(node, ts.SyntaxKind.CommaToken, sourceFile, 'a fixed-array type comma');
    typeKind = {
      kind: 'fixed-array', keyword_span: name.span, less_than_span: nodeSpan(less, sourceFile, file),
      element, comma_span: nodeSpan(comma, sourceFile, file),
      length_span: nodeSpan(lengthNode.literal, sourceFile, file),
      length: Number(lengthSpelling), length_spelling: lengthSpelling,
      greater_than_span: nodeSpan(greater, sourceFile, file),
    };
  }
  const typeId = pushType(typeSyntax, budgets, {
    span: spanFromOffsets(sourceFile, file, node.expression.getStart(sourceFile), greater.getEnd()),
    kind: typeKind,
  });
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

export {
  normalizeArrayConstruction,
  normalizeConstructionFields
};
