import {
  maxFixedArrayLength,
  maxFixedArrayLengthSpellingBytes,
  maxTypeSyntaxNesting,
  maxTypeSyntaxNodesPerModule,
  maxTypeSyntaxNodesPerProject,
  ts,
} from '../boundary/configuration.mjs';
import {
  failBudget,
} from '../boundary/errors.mjs';
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

function pushType(typeSyntax, budgets, value) {
  if (typeSyntax.length >= maxTypeSyntaxNodesPerModule) failBudget('module exceeds the type-syntax limit');
  budgets.types += 1;
  if (budgets.types > maxTypeSyntaxNodesPerProject) failBudget('project exceeds the type-syntax limit');
  const id = typeSyntax.length;
  typeSyntax.push(value);
  return id;
}

function normalizeType(node, insertionOffset, sourceFile, file, collector, context, typeSyntax, budgets, depth = 1) {
  if (depth > maxTypeSyntaxNesting) failBudget('type syntax exceeds the nesting limit');
  if (!node) {
    return pushType(typeSyntax, budgets, {
      span: spanFromOffsets(sourceFile, file, insertionOffset, insertionOffset),
      kind: { kind: 'missing' },
    });
  }
  if (!ts.isTypeReferenceNode(node) || !ts.isIdentifier(node.typeName)) {
    collector.unsupported(node, sourceFile, file, context);
    return null;
  }
  const name = dataName(node.typeName, sourceFile, file, collector, context);
  if (!name) return null;
  const args = [...(node.typeArguments ?? [])];
  if (args.length === 0) {
    const kind = name.text === 'String'
      ? { kind: 'string', keyword_span: name.span }
      : { kind: 'named', name };
    return pushType(typeSyntax, budgets, { span: nodeSpan(node, sourceFile, file), kind });
  }
  const unary = new Map([
    ['Vec', 'vec'], ['Shared', 'shared'], ['Weak', 'weak'],
    ['Borrow', 'borrow'], ['BorrowMut', 'borrow-mut'],
  ]);
  const container = unary.get(name.text);
  if (container && args.length === 1) {
    const argument = normalizeType(args[0], args[0].getStart(sourceFile), sourceFile, file, collector, context, typeSyntax, budgets, depth + 1);
    const less = requiredToken(node, ts.SyntaxKind.LessThanToken, sourceFile, 'a type-argument open token');
    const greater = requiredToken(node, ts.SyntaxKind.GreaterThanToken, sourceFile, 'a type-argument close token');
    if (argument === null) return null;
    return pushType(typeSyntax, budgets, {
      span: nodeSpan(node, sourceFile, file),
      kind: {
        kind: container, keyword_span: name.span, less_than_span: nodeSpan(less, sourceFile, file),
        argument, greater_than_span: nodeSpan(greater, sourceFile, file),
      },
    });
  }
  if (name.text === 'FixedArray' && args.length === 2) {
    const element = normalizeType(args[0], args[0].getStart(sourceFile), sourceFile, file, collector, context, typeSyntax, budgets, depth + 1);
    const lengthNode = args[1];
    if (!ts.isLiteralTypeNode(lengthNode) || !ts.isNumericLiteral(lengthNode.literal)) {
      collector.unsupported(lengthNode, sourceFile, file, 'fixed-array length');
      return null;
    }
    const spelling = lengthNode.literal.getText(sourceFile);
    if (
      !/^(0|[1-9][0-9]*)$/.test(spelling) ||
      Buffer.byteLength(spelling, 'utf8') > maxFixedArrayLengthSpellingBytes ||
      BigInt(spelling) > BigInt(maxFixedArrayLength)
    ) {
      failBudget(`fixed-array length must be canonical and at most ${maxFixedArrayLength}`);
    }
    const less = requiredToken(node, ts.SyntaxKind.LessThanToken, sourceFile, 'a fixed-array type open token');
    const greater = requiredToken(node, ts.SyntaxKind.GreaterThanToken, sourceFile, 'a fixed-array type close token');
    const comma = requiredToken(node, ts.SyntaxKind.CommaToken, sourceFile, 'a fixed-array type comma');
    if (element === null) return null;
    return pushType(typeSyntax, budgets, {
      span: nodeSpan(node, sourceFile, file),
      kind: {
        kind: 'fixed-array', keyword_span: name.span, less_than_span: nodeSpan(less, sourceFile, file),
        element, comma_span: nodeSpan(comma, sourceFile, file),
        length_span: nodeSpan(lengthNode.literal, sourceFile, file),
        length: Number(spelling), length_spelling: spelling,
        greater_than_span: nodeSpan(greater, sourceFile, file),
      },
    });
  }
  collector.unsupported(node, sourceFile, file, context);
  return null;
}

export {
  normalizeType,
  pushType
};
