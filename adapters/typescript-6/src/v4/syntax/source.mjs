import {
  maxFunctionsPerFile,
  maxFunctionsPerProject,
  maxImportsPerFile,
  maxImportsPerProject,
  maxNominalDeclarationsPerModule,
  maxSourceBytes,
  maxSourceFileBytes,
  ts,
} from '../boundary/configuration.mjs';
import {
  AdapterError,
  failBudget,
  failRequest,
} from '../boundary/errors.mjs';
import {
  normalizeDataDeclaration,
} from './data-declarations.mjs';
import {
  compactText,
} from './diagnostics.mjs';
import {
  normalizeFunction,
} from './functions.mjs';
import {
  normalizeImport,
} from './imports.mjs';
import {
  buildUtf8OffsetMap,
  spanFromOffsets,
} from './spans.mjs';
import {
  enforceParserNesting,
} from './tokens.mjs';

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

function normalizeSource(input, file, collector, budgets) {
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

export {
  normalizeSource
};
