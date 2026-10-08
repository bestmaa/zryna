import {
  maxMembersPerDeclaration,
  maxMembersPerProject,
  maxNominalDeclarationsPerProject,
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
} from './spans.mjs';
import {
  requiredToken,
} from './tokens.mjs';
import {
  normalizeType,
} from './types.mjs';

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
  if (node.typeParameters?.length || node.heritageClauses?.length !== 1) {
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
    const semicolon = requiredToken(member, ts.SyntaxKind.SemicolonToken, sourceFile, 'a data member semicolon');
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
  return { span: nodeSpan(node, sourceFile, file), export_span: exportSpan, kind: common };
}

export {
  normalizeDataDeclaration
};
