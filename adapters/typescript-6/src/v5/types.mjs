// Isolated protocol-v5 types; inherited v4 forms retain their source-only meaning.
import ts from '@typescript/typescript6';
import {
  maxTypeSyntaxNodesPerModule,
  maxTypeSyntaxNodesPerProject,
  maxTypeSyntaxNesting,
  maxFixedArrayLength,
  maxFixedArrayLengthSpellingBytes,
  failBudget,
  failInvariant,
  spanFromOffsets,
  nodeSpan,
  normalizedIdentifier,
  requiredToken,
  dataName
} from './common.mjs';

export function pushType(typeSyntax, budgets, value) {
  if (typeSyntax.length >= maxTypeSyntaxNodesPerModule) failBudget('module exceeds the type-syntax limit');
  budgets.types += 1;
  if (budgets.types > maxTypeSyntaxNodesPerProject) failBudget('project exceeds the type-syntax limit');
  const id = typeSyntax.length;
  typeSyntax.push(value);
  return id;
}

export function normalizeType(node, insertionOffset, sourceFile, file, collector, context, typeSyntax, budgets, depth = 1) {
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
    const commaSpan = genericList(node, node.typeArguments, sourceFile, file, collector, {}).comma_spans[0];
    if (element === null) return null;
    return pushType(typeSyntax, budgets, {
      span: nodeSpan(node, sourceFile, file),
      kind: {
        kind: 'fixed-array', keyword_span: name.span, less_than_span: nodeSpan(less, sourceFile, file),
        element, comma_span: commaSpan,
        length_span: nodeSpan(lengthNode.literal, sourceFile, file),
        length: Number(spelling), length_spelling: spelling,
        greater_than_span: nodeSpan(greater, sourceFile, file),
      },
    });
  }
  const typeArguments = normalizeTypeArguments(node, sourceFile, file, collector, typeSyntax, budgets, depth);
  return pushType(typeSyntax, budgets, { span: nodeSpan(node, sourceFile, file),
    kind: { kind: 'application', name, type_arguments: typeArguments } });
}

export function normalizeParameter(parameter, sourceFile, file, collector, index, typeSyntax, budgets) {
  let valid = true;
  let name = null;
  if (!ts.isIdentifier(parameter.name) || parameter.name.text === 'this') {
    collector.unsupported(parameter.name, sourceFile, file, `parameter ${index} name`);
    valid = false;
  } else {
    name = normalizedIdentifier(parameter.name, sourceFile, file, collector, `parameter ${index} name`);
    valid &&= name !== null;
  }
  for (const token of [parameter.dotDotDotToken, parameter.questionToken, parameter.initializer, ...(parameter.modifiers ?? [])]) {
    if (token) {
      collector.unsupported(token, sourceFile, file, `parameter ${index}`);
      valid = false;
    }
  }
  const typeId = normalizeType(parameter.type, parameter.name.getEnd(), sourceFile, file, collector, `parameter ${index} annotation`, typeSyntax, budgets);
  valid &&= typeId !== null;
  return valid ? { span: nodeSpan(parameter, sourceFile, file), name, type_syntax: typeId } : null;
}


export function genericList(node, items, sourceFile, file, collector, values) {
  if (items.length < 1 || items.length > 2) {
    collector.unsupported(node, sourceFile, file, 'bounded type list');
  }
  const children = node.getChildren(sourceFile);
  const less = children.find(child => child.kind === ts.SyntaxKind.LessThanToken);
  const greater = children.find(child => child.kind === ts.SyntaxKind.GreaterThanToken);
  if (!less || !greater) failInvariant('TypeScript omitted generic list punctuation');
  const list = children.find(child => child.kind === ts.SyntaxKind.SyntaxList &&
    child.pos >= less.end && child.end <= greater.pos);
  if (!list) failInvariant('TypeScript omitted generic syntax list');
  return {
    span: spanFromOffsets(sourceFile, file, less.getStart(sourceFile), greater.getEnd()),
    less_than_span: nodeSpan(less, sourceFile, file), ...values,
    comma_spans: list.getChildren(sourceFile).filter(child => child.kind === ts.SyntaxKind.CommaToken)
      .map(child => nodeSpan(child, sourceFile, file)),
    greater_than_span: nodeSpan(greater, sourceFile, file),
  };
}

export function normalizeTypeArguments(node, sourceFile, file, collector, typeSyntax, budgets, depth = 0) {
  if (!node.typeArguments) return null;
  if (node.typeArguments.length < 1 || node.typeArguments.length > 2) {
    collector.unsupported(node, sourceFile, file, 'bounded type arguments');
  }
  const arguments_ = node.typeArguments.map(argument => normalizeType(argument,
    argument.getStart(sourceFile), sourceFile, file, collector, 'type argument',
    typeSyntax, budgets, depth + 1));
  return genericList(node, node.typeArguments, sourceFile, file, collector, { arguments: arguments_ });
}

export function normalizeTypeParameters(node, sourceFile, file, collector) {
  if (!node.typeParameters) return null;
  if (node.typeParameters.length < 1 || node.typeParameters.length > 2) {
    collector.unsupported(node, sourceFile, file, 'bounded type parameters');
  }
  const parameters = node.typeParameters.map(parameter => {
    if (parameter.modifiers?.length || parameter.default ||
      !parameter.constraint || !ts.isTypeReferenceNode(parameter.constraint) ||
      !ts.isIdentifier(parameter.constraint.typeName) || parameter.constraint.typeArguments) {
      collector.unsupported(parameter, sourceFile, file, 'bounded type parameter');
    }
    const extendsToken = requiredToken(parameter, ts.SyntaxKind.ExtendsKeyword, sourceFile, 'bound extends');
    return {
      span: nodeSpan(parameter, sourceFile, file),
      name: normalizedIdentifier(parameter.name, sourceFile, file, collector, 'type parameter'),
      extends_span: nodeSpan(extendsToken, sourceFile, file),
      bound: normalizedIdentifier(parameter.constraint.typeName, sourceFile, file, collector, 'type bound'),
    };
  });
  return genericList(node, node.typeParameters, sourceFile, file, collector, { parameters });
}
