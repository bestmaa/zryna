import process from 'node:process';
import ts from '@typescript/typescript6';
import { PROTOCOL_V4_LIMITS } from '../../limits-v4.mjs';

function boundedTestLimit(name, productionLimit) {
  if (process.env.NODE_ENV !== 'test') return productionLimit;
  const configured = process.env[`ZRYNA_TEST_${name}`];
  if (configured === undefined) return productionLimit;
  const value = Number(configured);
  if (!Number.isSafeInteger(value) || value < 1 || value > productionLimit) {
    throw new Error(`invalid lowered test limit for ${name}`);
  }
  return value;
}

const protocolVersion = 4;
const expectedProviderVersion = '6.0.3';
const providerVersion = ts.version;
if (providerVersion !== expectedProviderVersion) {
  throw new Error(`the TypeScript provider must be exactly ${expectedProviderVersion}`);
}

const maxRequestBytes = boundedTestLimit('REQUEST_BYTES', PROTOCOL_V4_LIMITS.requestBytes);
const maxResponseBytes = boundedTestLimit('RESPONSE_BYTES', PROTOCOL_V4_LIMITS.responseBytes);
const maxFiles = boundedTestLimit('FILES', PROTOCOL_V4_LIMITS.files);
const maxSourceFileBytes = boundedTestLimit('SOURCE_FILE_BYTES', PROTOCOL_V4_LIMITS.sourceFileBytes);
const maxSourceBytes = boundedTestLimit('SOURCE_BYTES', PROTOCOL_V4_LIMITS.sourceBytes);
const maxFunctionsPerFile = boundedTestLimit('FUNCTIONS_PER_FILE', 4096);
const maxFunctionsPerProject = boundedTestLimit('FUNCTIONS_PER_PROJECT', 16_384);
const maxImportsPerFile = boundedTestLimit('IMPORTS_PER_FILE', 4096);
const maxImportsPerProject = boundedTestLimit('IMPORTS_PER_PROJECT', 65_536);
const maxBindingsPerImport = boundedTestLimit('BINDINGS_PER_IMPORT', 256);
const maxBindingsPerProject = boundedTestLimit('BINDINGS_PER_PROJECT', 65_536);
const maxParametersPerFunction = boundedTestLimit('PARAMETERS_PER_FUNCTION', 256);
const maxParametersPerProject = boundedTestLimit('PARAMETERS_PER_PROJECT', 262_144);
const maxBlocksPerFunction = boundedTestLimit('BLOCKS_PER_FUNCTION', 4096);
const maxBlocksPerProject = boundedTestLimit('BLOCKS_PER_PROJECT', 65_536);
const maxStatementsPerFunction = boundedTestLimit('STATEMENTS_PER_FUNCTION', 4096);
const maxStatementsPerProject = boundedTestLimit('STATEMENTS_PER_PROJECT', 65_536);
const maxExpressionsPerFunction = boundedTestLimit('EXPRESSIONS_PER_FUNCTION', 16_384);
const maxExpressionsPerProject = boundedTestLimit('EXPRESSIONS_PER_PROJECT', 262_144);
const maxLocalsPerFunction = boundedTestLimit('LOCALS_PER_FUNCTION', 4096);
const maxLocalsPerProject = boundedTestLimit('LOCALS_PER_PROJECT', 65_536);
const maxNesting = boundedTestLimit('NESTING', 128);
const maxCallArguments = boundedTestLimit('CALL_ARGUMENTS', 256);
const maxNominalDeclarationsPerModule = boundedTestLimit(
  'NOMINAL_DECLARATIONS_PER_MODULE',
  boundedTestLimit('NOMINAL_DECLARATIONS', PROTOCOL_V4_LIMITS.nominalDeclarationsPerModule),
);
const maxNominalDeclarationsPerProject = boundedTestLimit('NOMINAL_DECLARATIONS_PER_PROJECT', PROTOCOL_V4_LIMITS.nominalDeclarationsPerProject);
const maxMembersPerDeclaration = boundedTestLimit('MEMBERS_PER_DECLARATION', PROTOCOL_V4_LIMITS.membersPerDeclaration);
const maxMembersPerProject = boundedTestLimit('MEMBERS_PER_PROJECT', PROTOCOL_V4_LIMITS.membersPerProject);
const maxTypeSyntaxNodesPerModule = boundedTestLimit(
  'TYPE_SYNTAX_NODES_PER_MODULE',
  boundedTestLimit('TYPE_SYNTAX_NODES', PROTOCOL_V4_LIMITS.typeSyntaxNodesPerModule),
);
const maxTypeSyntaxNodesPerProject = boundedTestLimit('TYPE_SYNTAX_NODES_PER_PROJECT', PROTOCOL_V4_LIMITS.typeSyntaxNodesPerProject);
const maxTypeSyntaxNesting = boundedTestLimit('TYPE_SYNTAX_NESTING', PROTOCOL_V4_LIMITS.typeSyntaxNesting);
const maxObjectInitializersPerConstruction = boundedTestLimit('OBJECT_INITIALIZERS_PER_CONSTRUCTION', PROTOCOL_V4_LIMITS.objectInitializersPerConstruction);
const maxArrayElementsPerConstruction = boundedTestLimit('ARRAY_ELEMENTS_PER_CONSTRUCTION', PROTOCOL_V4_LIMITS.arrayElementsPerConstruction);
const maxConstructionOperandsPerProject = boundedTestLimit(
  'CONSTRUCTION_OPERANDS_PER_PROJECT',
  boundedTestLimit('AGGREGATE_OPERANDS', PROTOCOL_V4_LIMITS.constructionOperandsPerProject),
);
const maxMatchArmsPerExpression = boundedTestLimit('MATCH_ARMS_PER_EXPRESSION', PROTOCOL_V4_LIMITS.matchArmsPerExpression);
const maxMatchArmsPerProject = boundedTestLimit('MATCH_ARMS_PER_PROJECT', PROTOCOL_V4_LIMITS.matchArmsPerProject);
const maxFixedArrayLength = PROTOCOL_V4_LIMITS.fixedArrayLength;
const maxFixedArrayLengthSpellingBytes = PROTOCOL_V4_LIMITS.fixedArrayLengthSpellingBytes;
const maxDiagnostics = PROTOCOL_V4_LIMITS.diagnostics;
const maxDiagnosticCharacters = 4096;
const maxIdentifierBytes = 128;
const maxIntegerSpellingBytes = 64;
const maxJsonDepth = 8;
const maxJsonContainers = maxFiles + 4;
const maxJsonFields = maxFiles * 2 + 8;

export {
  maxArrayElementsPerConstruction,
  maxBindingsPerImport,
  maxBindingsPerProject,
  maxBlocksPerFunction,
  maxBlocksPerProject,
  maxCallArguments,
  maxConstructionOperandsPerProject,
  maxDiagnosticCharacters,
  maxDiagnostics,
  maxExpressionsPerFunction,
  maxExpressionsPerProject,
  maxFiles,
  maxFixedArrayLength,
  maxFixedArrayLengthSpellingBytes,
  maxFunctionsPerFile,
  maxFunctionsPerProject,
  maxIdentifierBytes,
  maxImportsPerFile,
  maxImportsPerProject,
  maxIntegerSpellingBytes,
  maxJsonContainers,
  maxJsonDepth,
  maxJsonFields,
  maxLocalsPerFunction,
  maxLocalsPerProject,
  maxMatchArmsPerExpression,
  maxMatchArmsPerProject,
  maxMembersPerDeclaration,
  maxMembersPerProject,
  maxNesting,
  maxNominalDeclarationsPerModule,
  maxNominalDeclarationsPerProject,
  maxObjectInitializersPerConstruction,
  maxParametersPerFunction,
  maxParametersPerProject,
  maxRequestBytes,
  maxResponseBytes,
  maxSourceBytes,
  maxSourceFileBytes,
  maxStatementsPerFunction,
  maxStatementsPerProject,
  maxTypeSyntaxNesting,
  maxTypeSyntaxNodesPerModule,
  maxTypeSyntaxNodesPerProject,
  protocolVersion,
  providerVersion,
  ts
};
