// Isolated protocol-v5 declarations; inherited v4 forms retain their source-only meaning.
import ts from '@typescript/typescript6';
import {
  maxSourceFileBytes,
  maxSourceBytes,
  maxFunctionsPerFile,
  maxFunctionsPerProject,
  maxImportsPerFile,
  maxImportsPerProject,
  maxBindingsPerImport,
  maxBindingsPerProject,
  maxParametersPerFunction,
  maxParametersPerProject,
  maxNominalDeclarationsPerModule,
  maxNominalDeclarationsPerProject,
  maxMembersPerDeclaration,
  maxMembersPerProject,
  AdapterError,
  failRequest,
  failBudget,
  buildUtf8OffsetMap,
  spanFromOffsets,
  nodeSpan,
  compactText,
  normalizedIdentifier,
  findToken,
  requiredToken,
  enforceParserNesting,
  dataName
} from './common.mjs';
import {
  normalizeType,
  normalizeParameter,
  normalizeTypeParameters
} from './types.mjs';
import {
  allocateBlock
} from './statements.mjs';

function normalizeImport(node, sourceFile, file, collector, budgets) {
  let valid = true;
  if (!node.importClause || node.importClause.isTypeOnly || node.importClause.name || !node.importClause.namedBindings || !ts.isNamedImports(node.importClause.namedBindings) || node.attributes) {
    collector.unsupported(node, sourceFile, file, 'import declaration');
    return null;
  }
  if (!ts.isStringLiteral(node.moduleSpecifier)) {
    collector.unsupported(node.moduleSpecifier, sourceFile, file, 'module specifier');
    return null;
  }
  const elements = node.importClause.namedBindings.elements;
  if (elements.length === 0) {
    collector.unsupported(node.importClause.namedBindings, sourceFile, file, 'empty named import');
    valid = false;
  }
  if (elements.length > maxBindingsPerImport) failBudget('import exceeds the imported-name limit');
  budgets.bindings += elements.length;
  if (budgets.bindings > maxBindingsPerProject) failBudget('project exceeds the imported-name limit');
  const raw = node.moduleSpecifier.getText(sourceFile);
  const quote = raw[0];
  const value = node.moduleSpecifier.text;
  const components = value.split('/');
  const specifierBody = value.startsWith('./')
    ? value.slice(2)
    : value.replace(/^(?:\.\.\/)+/, '');
  const validSpecifier =
    (value.startsWith('./') || value.startsWith('../')) &&
    value.endsWith('.zry') &&
    /^[\x00-\x7f]*$/.test(value) &&
    !value.includes('\\') &&
    !value.includes('\0') &&
    !value.includes('?') &&
    !value.includes('#') &&
    !value.includes('://') &&
    specifierBody.length > 0 &&
    components.every((component) => component.length > 0);
  if ((quote !== '"' && quote !== "'") || raw.at(-1) !== quote || raw.slice(1, -1) !== value || raw.includes('\\') || Buffer.byteLength(value, 'utf8') > 1024 || !validSpecifier) {
    collector.unsupported(node.moduleSpecifier, sourceFile, file, 'module specifier');
    valid = false;
  }
  const bindings = [];
  for (const element of elements) {
    if (element.isTypeOnly) {
      collector.unsupported(element, sourceFile, file, 'type-only import');
      valid = false;
      continue;
    }
    const importedNode = element.propertyName ?? element.name;
    const imported = normalizedIdentifier(importedNode, sourceFile, file, collector, 'imported name');
    const local = normalizedIdentifier(element.name, sourceFile, file, collector, 'import local name');
    const asToken = element.propertyName ? requiredToken(element, ts.SyntaxKind.AsKeyword, sourceFile, 'an import alias keyword') : null;
    if (!imported || !local) valid = false;
    else bindings.push({ span: nodeSpan(element, sourceFile, file), imported, local, as_span: asToken ? nodeSpan(asToken, sourceFile, file) : null });
  }
  const importToken = requiredToken(node, ts.SyntaxKind.ImportKeyword, sourceFile, 'an import keyword');
  const fromToken = requiredToken(node, ts.SyntaxKind.FromKeyword, sourceFile, 'an import from keyword');
  const semicolon = findToken(node, ts.SyntaxKind.SemicolonToken, sourceFile);
  if (!semicolon) {
    collector.unsupported(node, sourceFile, file, 'import without a semicolon');
    valid = false;
  }
  if (!valid || !semicolon) return null;
  const tokenStart = node.moduleSpecifier.getStart(sourceFile);
  return {
    span: nodeSpan(node, sourceFile, file),
    import_span: nodeSpan(importToken, sourceFile, file),
    bindings,
    from_span: nodeSpan(fromToken, sourceFile, file),
    specifier: {
      text: value,
      token_span: nodeSpan(node.moduleSpecifier, sourceFile, file),
      value_span: spanFromOffsets(sourceFile, file, tokenStart + 1, node.moduleSpecifier.getEnd() - 1),
    },
    semicolon_span: nodeSpan(semicolon, sourceFile, file),
  };
}

function normalizeDataDeclaration(node, sourceFile, file, collector, budgets, typeSyntax) {
  let exportSpan = null;
  for (const modifier of node.modifiers ?? []) {
    if (modifier.kind === ts.SyntaxKind.ExportKeyword && exportSpan === null) {
      exportSpan = nodeSpan(modifier, sourceFile, file);
    } else {
      collector.unsupported(modifier, sourceFile, file, 'data declaration modifier');
      return null;
    }
  }
  if (node.heritageClauses?.length !== 1) {
    collector.unsupported(node, sourceFile, file, 'data declaration');
    return null;
  }
  const heritage = node.heritageClauses[0];
  const markerType = heritage.types?.[0];
  if (
    heritage.token !== ts.SyntaxKind.ExtendsKeyword || heritage.types.length !== 1 ||
    !markerType || !ts.isIdentifier(markerType.expression) || markerType.typeArguments?.length
  ) {
    collector.unsupported(heritage, sourceFile, file, 'data declaration marker');
    return null;
  }
  const marker = markerType.expression.text;
  if (marker !== 'ZrynaStruct' && marker !== 'ZrynaEnum') {
    collector.unsupported(markerType, sourceFile, file, 'data declaration marker');
    return null;
  }
  const typeParameters = normalizeTypeParameters(node, sourceFile, file, collector);
  const name = dataName(node.name, sourceFile, file, collector, 'data declaration name');
  if (!name) return null;
  if (node.members.length === 0) {
    collector.unsupported(node, sourceFile, file, 'empty data declaration');
    return null;
  }
  if (node.members.length > maxMembersPerDeclaration) {
    failBudget('data declaration exceeds the member limit');
  }
  budgets.members += node.members.length;
  if (budgets.members > maxMembersPerProject) failBudget('project exceeds the data-member limit');
  const seen = new Set();
  const members = [];
  for (const member of node.members) {
    if (
      !ts.isPropertySignature(member) || !member.type || !member.name ||
      !ts.isIdentifier(member.name) || member.questionToken || member.modifiers?.length
    ) {
      collector.unsupported(member, sourceFile, file, 'data member');
      return null;
    }
    const memberName = dataName(member.name, sourceFile, file, collector, 'data member name');
    if (!memberName || seen.has(memberName.text)) {
      collector.unsupported(member.name, sourceFile, file, 'duplicate or invalid data member name');
      return null;
    }
    seen.add(memberName.text);
    const colon = requiredToken(member, ts.SyntaxKind.ColonToken, sourceFile, 'a data member colon');
    const semicolon = findToken(member, ts.SyntaxKind.SemicolonToken, sourceFile);
    if (!semicolon) collector.unsupported(member, sourceFile, file, 'data member without a semicolon');
    const base = {
      span: nodeSpan(member, sourceFile, file), name: memberName,
      colon_span: nodeSpan(colon, sourceFile, file), semicolon_span: nodeSpan(semicolon, sourceFile, file),
    };
    if (marker === 'ZrynaEnum' && ts.isTypeReferenceNode(member.type) && ts.isIdentifier(member.type.typeName) && member.type.typeName.text === 'ZrynaNone' && !member.type.typeArguments?.length) {
      members.push({ ...base, payload_type: null, none_span: nodeSpan(member.type, sourceFile, file) });
    } else {
      const typeId = normalizeType(member.type, member.name.getEnd(), sourceFile, file, collector, 'data member type', typeSyntax, budgets);
      if (typeId === null) return null;
      if (marker === 'ZrynaEnum') members.push({ ...base, payload_type: typeId, none_span: null });
      else members.push({ ...base, type_syntax: typeId });
    }
  }
  budgets.dataDeclarations += 1;
  if (budgets.dataDeclarations > maxNominalDeclarationsPerProject) failBudget('project exceeds the nominal-declaration limit');
  const interfaceToken = requiredToken(node, ts.SyntaxKind.InterfaceKeyword, sourceFile, 'an interface keyword');
  const extendsToken = requiredToken(heritage, ts.SyntaxKind.ExtendsKeyword, sourceFile, 'an extends keyword');
  const open = requiredToken(node, ts.SyntaxKind.OpenBraceToken, sourceFile, 'a data declaration open brace');
  const close = requiredToken(node, ts.SyntaxKind.CloseBraceToken, sourceFile, 'a data declaration close brace');
  const common = {
    kind: marker === 'ZrynaStruct' ? 'struct' : 'enum',
    interface_span: nodeSpan(interfaceToken, sourceFile, file), name,
    extends_span: nodeSpan(extendsToken, sourceFile, file),
    marker_span: nodeSpan(markerType.expression, sourceFile, file),
    open_brace_span: nodeSpan(open, sourceFile, file), close_brace_span: nodeSpan(close, sourceFile, file),
  };
  if (marker === 'ZrynaStruct') common.fields = members;
  else common.variants = members;
  return { span: nodeSpan(node, sourceFile, file), export_span: exportSpan, type_parameters: typeParameters, kind: common };
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
  if (node.asteriskToken || !node.name || !ts.isIdentifier(node.name) || !node.body) {
    collector.unsupported(node, sourceFile, file, `function ${index}`);
    valid = false;
  }
  const typeParameters = normalizeTypeParameters(node, sourceFile, file, collector);
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
    type_parameters: typeParameters, parameters, result_type: resultType,
    body: { span: nodeSpan(node.body, sourceFile, file), root_block: rootBlock, blocks: context.blocks, statements: context.statements, expressions: context.expressions },
  };
}

function addParseDiagnostics(sourceFile, file, collector) {
  const diagnostics = [...sourceFile.parseDiagnostics].sort((a, b) => (a.start ?? -1) - (b.start ?? -1) || a.code - b.code);
  if (diagnostics.length > 0) {
    const diagnostic = diagnostics[0];
    const start = diagnostic.start ?? 0;
    const end = Math.min(sourceFile.text.length, start + (diagnostic.length ?? 0));
    const span = spanFromOffsets(sourceFile, file, start, end);
    throw new AdapterError(
      'ZRYNA-F2002',
      `TypeScript parse error TS${diagnostic.code} at file ${file} bytes ${span.start}..${span.end}: ${compactText(ts.flattenDiagnosticMessageText(diagnostic.messageText, ' '))}`,
    );
  }
}

export function normalizeSource(input, file, collector, budgets) {
  if (!input.text.isWellFormed()) failRequest(`source '${input.path}' contains an unpaired UTF-16 surrogate`);
  if (input.text.startsWith('#!')) {
    throw new AdapterError('ZRYNA-F2002', `source '${input.path}' uses an unsupported hashbang`);
  }
  const bytes = Buffer.byteLength(input.text, 'utf8');
  if (bytes > maxSourceFileBytes) failBudget('source file exceeds the byte limit');
  budgets.sourceBytes += bytes;
  if (budgets.sourceBytes > maxSourceBytes) failBudget('project exceeds the source byte limit');
  enforceParserNesting(input.text);
  let sourceFile;
  try {
    sourceFile = ts.createSourceFile(input.path, input.text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  } catch (error) {
    if (error instanceof RangeError) failBudget('source exceeds TypeScript parser capacity');
    throw error;
  }
  buildUtf8OffsetMap(sourceFile);
  addParseDiagnostics(sourceFile, file, collector);
  const imports = [];
  const typeSyntax = [];
  const dataDeclarations = [];
  const functions = [];
  let sawNonImport = false;
  let importDeclarations = 0;
  let functionDeclarations = 0;
  for (const node of sourceFile.statements) {
    if (ts.isImportDeclaration(node)) {
      if (sawNonImport) {
        collector.unsupported(node, sourceFile, file, 'import after declaration');
        continue;
      }
      importDeclarations += 1;
      if (importDeclarations > maxImportsPerFile) failBudget('module exceeds the import-declaration limit');
      budgets.imports += 1;
      if (budgets.imports > maxImportsPerProject) failBudget('project exceeds the import-declaration limit');
      const normalized = normalizeImport(node, sourceFile, file, collector, budgets);
      if (normalized) imports.push(normalized);
      continue;
    }
    sawNonImport = true;
    if (ts.isInterfaceDeclaration(node)) {
      if (dataDeclarations.length >= maxNominalDeclarationsPerModule) {
        failBudget('module exceeds the nominal-declaration limit');
      }
      const normalized = normalizeDataDeclaration(node, sourceFile, file, collector, budgets, typeSyntax);
      if (normalized) dataDeclarations.push(normalized);
      continue;
    }
    if (!ts.isFunctionDeclaration(node)) {
      collector.unsupported(node, sourceFile, file, 'top-level declaration');
      continue;
    }
    functionDeclarations += 1;
    if (functionDeclarations > maxFunctionsPerFile) failBudget('module exceeds the function limit');
    budgets.functions += 1;
    if (budgets.functions > maxFunctionsPerProject) failBudget('project exceeds the function limit');
    const normalized = normalizeFunction(node, sourceFile, file, collector, budgets, typeSyntax, functionDeclarations - 1);
    if (normalized) functions.push(normalized);
  }
  return { id: file, path: input.path, imports, type_syntax: typeSyntax, data_declarations: dataDeclarations, functions };
}

