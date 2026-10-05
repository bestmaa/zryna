// Isolated protocol-v5 statements; inherited v4 forms retain their source-only meaning.
import ts from '@typescript/typescript6';
import {
  maxBlocksPerFunction,
  maxBlocksPerProject,
  maxStatementsPerFunction,
  maxStatementsPerProject,
  maxLocalsPerFunction,
  maxLocalsPerProject,
  maxNesting,
  failBudget,
  failInvariant,
  nodeSpan,
  normalizedIdentifier,
  findToken,
  requiredToken,
  dataName
} from './common.mjs';
import {
  normalizeType
} from './types.mjs';
import {
  normalizeExpression
} from './expressions.mjs';

function requireSemicolon(node, context, label) {
  const token = findToken(node, ts.SyntaxKind.SemicolonToken, context.sourceFile);
  if (!token) {
    context.collector.unsupported(node, context.sourceFile, context.file, `${label} without a semicolon`);
    return null;
  }
  return nodeSpan(token, context.sourceFile, context.file);
}

function pushStatement(context, statement) {
  if (context.statements.length >= maxStatementsPerFunction) failBudget('function exceeds the statement limit');
  context.budgets.statements += 1;
  if (context.budgets.statements > maxStatementsPerProject) failBudget('project exceeds the statement limit');
  const id = context.statements.length;
  context.statements.push(statement);
  return id;
}

export function allocateBlock(block, context, depth) {
  const { sourceFile, file, collector } = context;
  if (depth > maxNesting) {
    collector.unsupported(block, sourceFile, file, 'block nesting');
    return null;
  }
  if (context.blocks.length >= maxBlocksPerFunction) failBudget('function exceeds the lexical-block limit');
  context.budgets.blocks += 1;
  if (context.budgets.blocks > maxBlocksPerProject) failBudget('project exceeds the lexical-block limit');
  const open = requiredToken(block, ts.SyntaxKind.OpenBraceToken, sourceFile, 'a block open brace');
  const close = requiredToken(block, ts.SyntaxKind.CloseBraceToken, sourceFile, 'a block close brace');
  const id = context.blocks.length;
  const output = { span: nodeSpan(block, sourceFile, file), open_brace_span: nodeSpan(open, sourceFile, file), statements: [], close_brace_span: nodeSpan(close, sourceFile, file) };
  context.blocks.push(output);
  for (const statement of block.statements) {
    const normalized = normalizeStatement(statement, context, depth);
    if (normalized === null) context.valid = false;
    else output.statements.push(normalized);
  }
  return id;
}

function normalizeStatement(node, context, depth) {
  const { sourceFile, file, collector } = context;
  const placeholder = pushStatement(context, null);
  let kind = null;
  if (ts.isVariableStatement(node)) {
    const declarationList = node.declarationList;
    const mutable = (declarationList.flags & ts.NodeFlags.Let) !== 0;
    const isConst = (declarationList.flags & ts.NodeFlags.Const) !== 0;
    const declaration = declarationList.declarations[0];
    if ((!mutable && !isConst) || declarationList.declarations.length !== 1 || !declaration || !ts.isIdentifier(declaration.name) || !declaration.type || !declaration.initializer) {
      collector.unsupported(node, sourceFile, file, 'local declaration');
    } else if (node.modifiers?.length || declaration.dotDotDotToken || declaration.exclamationToken) {
      collector.unsupported(node, sourceFile, file, 'local declaration');
    } else {
      context.locals += 1;
      context.budgets.locals += 1;
      if (context.locals > maxLocalsPerFunction) failBudget('function exceeds the local limit');
      if (context.budgets.locals > maxLocalsPerProject) failBudget('project exceeds the local limit');
      const keywordKind = mutable ? ts.SyntaxKind.LetKeyword : ts.SyntaxKind.ConstKeyword;
      const keyword = requiredToken(declarationList, keywordKind, sourceFile, 'a local declaration keyword');
      const equals = requiredToken(declaration, ts.SyntaxKind.EqualsToken, sourceFile, 'a local declaration equals token');
      const semicolon = requireSemicolon(node, context, 'local declaration');
      const name = normalizedIdentifier(declaration.name, sourceFile, file, collector, 'local name');
      const typeSyntax = normalizeType(
        declaration.type, declaration.name.getEnd(), sourceFile, file, collector,
        'local annotation', context.typeSyntax, context.budgets,
      );
      const initializer = normalizeExpression(declaration.initializer, context, depth + 1);
      if (semicolon && name && typeSyntax !== null && initializer !== null) kind = { kind: 'local-declaration', keyword_span: nodeSpan(keyword, sourceFile, file), mutable, name, type_syntax: typeSyntax, equals_span: nodeSpan(equals, sourceFile, file), initializer, semicolon_span: semicolon };
    }
  } else if (
    ts.isExpressionStatement(node) && ts.isCallExpression(node.expression) &&
    ts.isIdentifier(node.expression.expression) && node.expression.expression.text === 'upgradeWeak'
  ) {
    const call = node.expression;
    const semicolon = requireSemicolon(node, context, 'weak upgrade');
    if (
      call.questionDotToken || call.typeArguments?.length || call.arguments.length !== 3 ||
      !ts.isArrowFunction(call.arguments[1]) || !ts.isArrowFunction(call.arguments[2])
    ) {
      collector.unsupported(call, sourceFile, file, 'weak upgrade');
    } else {
      const success = call.arguments[1];
      const failure = call.arguments[2];
      const successParameter = success.parameters[0];
      const validSuccess =
        success.parameters.length === 1 && successParameter && ts.isIdentifier(successParameter.name) &&
        !successParameter.type && !successParameter.initializer && !successParameter.questionToken &&
        !successParameter.dotDotDotToken && !successParameter.modifiers?.length && ts.isBlock(success.body) &&
        !success.modifiers?.length && !success.typeParameters?.length && !success.type;
      const validFailure =
        failure.parameters.length === 0 && ts.isBlock(failure.body) &&
        !failure.modifiers?.length && !failure.typeParameters?.length && !failure.type;
      const asToken = requiredToken(success, ts.SyntaxKind.EqualsGreaterThanToken, sourceFile, 'a weak-upgrade success arrow');
      const elseToken = requiredToken(failure, ts.SyntaxKind.EqualsGreaterThanToken, sourceFile, 'a weak-upgrade failure arrow');
      const successParameters = sourceFile.text.slice(success.getStart(sourceFile), asToken.getStart(sourceFile)).trim();
      const failureParameters = sourceFile.text.slice(failure.getStart(sourceFile), elseToken.getStart(sourceFile)).trim();
      if (
        !validSuccess || !validFailure ||
        !successParameters.startsWith('(') || !successParameters.endsWith(')') ||
        !failureParameters.startsWith('(') || !failureParameters.endsWith(')')
      ) {
        collector.unsupported(call, sourceFile, file, 'weak upgrade callbacks');
      } else {
        const weak = normalizeExpression(call.arguments[0], context, depth + 1);
        const binding = dataName(successParameter.name, sourceFile, file, collector, 'weak upgrade binding');
        const successBlock = allocateBlock(success.body, context, depth + 1);
        const failureBlock = allocateBlock(failure.body, context, depth + 1);
        if (semicolon && weak !== null && binding && successBlock !== null && failureBlock !== null) {
          kind = {
            kind: 'weak-upgrade', keyword_span: nodeSpan(call.expression, sourceFile, file), weak,
            as_span: nodeSpan(asToken, sourceFile, file), binding, success_block: successBlock,
            else_span: nodeSpan(elseToken, sourceFile, file), failure_block: failureBlock,
          };
        }
      }
    }
  } else if (ts.isExpressionStatement(node) && ts.isBinaryExpression(node.expression) && node.expression.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
    const semicolon = requireSemicolon(node, context, 'assignment');
    const target = normalizeExpression(node.expression.left, context, depth + 1);
    const value = normalizeExpression(node.expression.right, context, depth + 1);
    if (semicolon && target !== null && value !== null) kind = { kind: 'assignment', target, equals_span: nodeSpan(node.expression.operatorToken, sourceFile, file), value, semicolon_span: semicolon };
  } else if (ts.isReturnStatement(node) && node.expression) {
    const semicolon = requireSemicolon(node, context, 'return');
    const value = normalizeExpression(node.expression, context, depth + 1);
    const keyword = requiredToken(node, ts.SyntaxKind.ReturnKeyword, sourceFile, 'a return keyword');
    if (semicolon && value !== null) kind = { kind: 'return', keyword_span: nodeSpan(keyword, sourceFile, file), value, semicolon_span: semicolon };
  } else if (ts.isBlock(node)) {
    const block = allocateBlock(node, context, depth + 1);
    if (block !== null) kind = { kind: 'block', block };
  } else if (ts.isIfStatement(node) && ts.isBlock(node.thenStatement) && (!node.elseStatement || ts.isBlock(node.elseStatement))) {
    const condition = normalizeExpression(node.expression, context, depth + 1);
    const thenBlock = allocateBlock(node.thenStatement, context, depth + 1);
    let elseClause = null;
    if (node.elseStatement) {
      const elseKeyword = node.elseStatement.getFullStart() >= 0 ? findToken(node, ts.SyntaxKind.ElseKeyword, sourceFile) : null;
      if (!elseKeyword) failInvariant('TypeScript omitted an else keyword');
      const block = allocateBlock(node.elseStatement, context, depth + 1);
      if (block !== null) elseClause = { keyword_span: nodeSpan(elseKeyword, sourceFile, file), block };
    }
    const keyword = requiredToken(node, ts.SyntaxKind.IfKeyword, sourceFile, 'an if keyword');
    const open = requiredToken(node, ts.SyntaxKind.OpenParenToken, sourceFile, 'an if open parenthesis');
    const close = requiredToken(node, ts.SyntaxKind.CloseParenToken, sourceFile, 'an if close parenthesis');
    if (condition !== null && thenBlock !== null && (!node.elseStatement || elseClause)) kind = { kind: 'if', keyword_span: nodeSpan(keyword, sourceFile, file), open_paren_span: nodeSpan(open, sourceFile, file), condition, close_paren_span: nodeSpan(close, sourceFile, file), then_block: thenBlock, else_clause: elseClause };
  } else if (ts.isWhileStatement(node) && ts.isBlock(node.statement)) {
    const condition = normalizeExpression(node.expression, context, depth + 1);
    const bodyBlock = allocateBlock(node.statement, context, depth + 1);
    const keyword = requiredToken(node, ts.SyntaxKind.WhileKeyword, sourceFile, 'a while keyword');
    const open = requiredToken(node, ts.SyntaxKind.OpenParenToken, sourceFile, 'a while open parenthesis');
    const close = requiredToken(node, ts.SyntaxKind.CloseParenToken, sourceFile, 'a while close parenthesis');
    if (condition !== null && bodyBlock !== null) kind = { kind: 'while', keyword_span: nodeSpan(keyword, sourceFile, file), open_paren_span: nodeSpan(open, sourceFile, file), condition, close_paren_span: nodeSpan(close, sourceFile, file), body_block: bodyBlock };
  } else if (ts.isExpressionStatement(node)) {
    const semicolon = requireSemicolon(node, context, 'expression statement');
    const expression = normalizeExpression(node.expression, context, depth + 1);
    if (semicolon && expression !== null) kind = { kind: 'expression-statement', expression, semicolon_span: semicolon };
  } else {
    collector.unsupported(node, sourceFile, file, 'statement');
  }
  if (kind === null) {
    context.statements[placeholder] = { span: nodeSpan(node, sourceFile, file), kind: { kind: 'invalid' } };
    return null;
  }
  context.statements[placeholder] = { span: nodeSpan(node, sourceFile, file), kind };
  return placeholder;
}

