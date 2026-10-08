import {
  maxParametersPerFunction,
  maxParametersPerProject,
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
} from './spans.mjs';
import {
  allocateBlock,
} from './statements.mjs';
import {
  requiredToken,
} from './tokens.mjs';
import {
  normalizeType,
} from './types.mjs';

function normalizeParameter(parameter, sourceFile, file, collector, index, typeSyntax, budgets) {
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

function normalizeFunction(node, sourceFile, file, collector, budgets, typeSyntax, index) {
  let valid = true;
  let exportSpan = null;
  for (const modifier of node.modifiers ?? []) {
    if (modifier.kind === ts.SyntaxKind.ExportKeyword && exportSpan === null) exportSpan = nodeSpan(modifier, sourceFile, file);
    else {
      collector.unsupported(modifier, sourceFile, file, `function ${index} modifier`);
      valid = false;
    }
  }
  if (node.asteriskToken || !node.name || !ts.isIdentifier(node.name) || node.typeParameters?.length || !node.body) {
    collector.unsupported(node, sourceFile, file, `function ${index}`);
    valid = false;
  }
  const name = node.name && ts.isIdentifier(node.name) ? normalizedIdentifier(node.name, sourceFile, file, collector, `function ${index} name`) : null;
  if (!name) valid = false;
  if (node.parameters.length > maxParametersPerFunction) failBudget('function exceeds the parameter limit');
  budgets.parameters += node.parameters.length;
  if (budgets.parameters > maxParametersPerProject) failBudget('project exceeds the parameter limit');
  const parameters = node.parameters.map((parameter, parameterIndex) => normalizeParameter(parameter, sourceFile, file, collector, parameterIndex, typeSyntax, budgets));
  if (parameters.some((parameter) => parameter === null)) valid = false;
  const resultType = normalizeType(node.type, node.parameters.end, sourceFile, file, collector, `function ${index} result annotation`, typeSyntax, budgets);
  if (resultType === null) valid = false;
  if (!node.body) return null;
  const context = { sourceFile, file, collector, budgets, typeSyntax, blocks: [], statements: [], expressions: [], locals: 0, valid: true };
  const rootBlock = allocateBlock(node.body, context, 1);
  valid &&= context.valid && rootBlock === 0;
  const functionToken = requiredToken(node, ts.SyntaxKind.FunctionKeyword, sourceFile, 'a function keyword');
  if (!valid || !name || resultType === null || rootBlock === null) return null;
  return {
    span: nodeSpan(node, sourceFile, file), export_span: exportSpan,
    function_span: nodeSpan(functionToken, sourceFile, file), name,
    parameters, result_type: resultType,
    body: { span: nodeSpan(node.body, sourceFile, file), root_block: rootBlock, blocks: context.blocks, statements: context.statements, expressions: context.expressions },
  };
}

export {
  normalizeFunction
};
