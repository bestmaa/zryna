"""Bind instruction/terminator getter facts to the pinned sealed operation structure."""
from debug_reader import fields, parse, argument
import json

ROLES = {'ModuleId':'module','ModuleIdentity':'module','ValueId':'value','ValueIdentity':'value',
         'PlaceId':'place','PlaceIdentity':'place','BorrowId':'borrow','BorrowIdentity':'borrow',
         'BlockId':'block','BlockIdentity':'block','CleanupPlanId':'cleanup','CleanupPlanIdentity':'cleanup',
         'TypeId':'type','LayoutTypeId':'type'}

def require(value, message):
    if not value:
        raise ValueError(message)

def normal(value):
    # This links differing raw/getter ID view types only. Retained observations and pair equality
    # remain byte-exact and preserve every nominal owner; this is not an IR equivalence serializer.
    if type(value) is list:
        return [normal(v) for v in value]
    if type(value) is not dict:
        return value
    tag = value.get('@tag')
    if tag in ROLES:
        number = argument(value) if '@args' in value else fields(value)['index']
        require(type(number) is int and number >= 0,'exact typed sealed/getter identity index')
        return (ROLES[tag],number)
    if tag in ('FunctionId','FunctionIdentity'):
        content = fields(value)
        module = normal(content['module']) if type(content['module']) is dict else ('module',content['module'])
        require(type(content['declaration']) is int and content['declaration'] >= 0,'exact function declaration index')
        require(type(module[1]) is int and module[1] >= 0,'exact function module index')
        return ('function',module,content['declaration'])
    if tag == 'VerifiedValueList':
        return normal(argument(value))
    return {name:normal(v) for name,v in value.items()}

def same(left,right):
    return json.dumps(left,sort_keys=True) == json.dumps(right,sort_keys=True)

def optional(value):
    require(type(value) is dict and value.get('@tag') in ('Some','None'),'exact optional getter tag')
    if value['@tag'] == 'None':
        require(set(value) == {'@tag'},'exact None getter')
        return None
    require(set(value) == {'@tag','@args'},'exact Some getter')
    return argument(value)

def m2_kind(kind):
    tag = kind['@tag']
    if '@args' in kind:
        return normal(kind)
    values = fields(kind)
    if tag in ('I32Add','I32Sub','I32Mul','Eq','Ne','I32LtS','I32LeS','I32GtS','I32GeS'):
        return {'@tag':tag,'@args':[normal(values['lhs']),normal(values['rhs'])]}
    if tag == 'I32Neg':
        return {'@tag':tag,'@args':[normal(values['operand'])]}
    return normal(kind)

def instruction(profile, raw, getter, span):
    source = fields(raw,'Instruction')
    kind = source['kind']
    if profile == 'm2':
        result = fields(source['result'],'ValueDefinition')
        require(normal(result['id']) == normal(parse(getter['result'])),'sealed instruction result identity')
        require(result['ty'] == parse(getter['type']) and span(result['span']) == getter['span'],'sealed instruction type/span')
        require(same(m2_kind(kind),normal(parse(getter['operation']))),'sealed complete M2 instruction operation')
        return
    require(span(source['span']) == getter['span'],'sealed instruction span')
    require(kind['@tag'] == getter['operation'],'sealed instruction opcode')
    result = optional(source['result'])
    projected = optional(parse(getter['result']))
    require((result is None) == (projected is None),'sealed optional instruction result')
    if result is not None:
        result = fields(result,'ValueDefinition')
        require(normal(result['id']) == normal(projected),'sealed instruction result identity')
        require(normal(result['ty']) == normal(optional(parse(getter['result_type']))),'sealed instruction result type')
    else:
        require(optional(parse(getter['result_type'])) is None,'sealed no-result instruction type')
    tag = kind['@tag']
    values = kind.get('@fields',{})
    require(type(getter['bool_literal']) is (bool if tag == 'BoolLiteral' else type(None)),'exact Bool literal type')
    require(type(getter['i32_literal']) is (int if tag == 'I32Literal' else type(None)),'exact i32 literal type')
    if getter['string_utf8_bytes'] is not None:
        require(type(getter['string_utf8_bytes']) is list and all(type(byte) is int and 0 <= byte <= 255 for byte in getter['string_utf8_bytes']),'exact literal byte types')
    require(getter['bool_literal'] == (argument(kind) if tag == 'BoolLiteral' else None),'sealed Bool literal')
    require(getter['i32_literal'] == (argument(kind) if tag == 'I32Literal' else None),'sealed i32 literal')
    require(getter['string_utf8_bytes'] == (values['bytes'] if tag == 'StringFromUtf8' else None),'sealed literal UTF-8 bytes')
    require(normal(optional(parse(getter['callee']))) == normal(values.get('callee')),'sealed direct callee')
    require(normal(parse(getter['call_arguments'])) == normal(values.get('arguments',[])),'sealed direct call arguments')
    variant = values.get('variant') if tag == 'EnumConstruct' else None
    require(type(getter['variant']) is (int if variant is not None else type(None)),'exact construction variant type')
    require(getter['variant'] == variant,'sealed construction variant')
    for raw_name,getter_name in (('element_cleanup','vec_clone_element_cleanup'),('element_cleanup','aggregate_clone_element_cleanup')):
        applicable = tag == ('VecClone' if getter_name.startswith('vec') else 'ClonePlace')
        expected = optional(values[raw_name]) if applicable and raw_name in values else None
        require(normal(optional(parse(getter[getter_name]))) == normal(expected),'sealed per-element cleanup role')
    cleanup = values.get('cleanup')
    if cleanup is None:
        cleanup = values.get('allocation_failure')
    # Cleanup roles differ for a few opcodes; the complete raw root still retains every field.
    if 'cleanup' in values:
        if cleanup.get('@tag') in ('None','Some'):
            cleanup = optional(cleanup)
        require(normal(optional(parse(getter['cleanup']))) == normal(cleanup),'sealed instruction cleanup role')

def terminator(profile, raw, getter, span):
    source = fields(raw,'SpannedTerminator')
    require(span(source['span']) == getter['span'],'sealed terminator span')
    kind = source['kind']
    if profile == 'm2':
        require(same(normal(kind),normal(parse(getter['operation']))),'sealed complete M2 terminator operation')
    else:
        require(kind['@tag'] == getter['operation'],'sealed terminator opcode')
        values = kind.get('@fields',{})
        require(normal(optional(parse(getter['cleanup']))) == normal(values.get('cleanup')),'sealed terminator cleanup')
        require(normal(optional(parse(getter['trap']))) == normal(values.get('identity')),'sealed typed trap')
        backend = normal(parse(getter['backend_view']))
        tag = kind['@tag']
        if tag == 'Return':
            expected = {'@tag':'Return','@args':[normal(values['value'])]}
        elif tag == 'Trap':
            expected = {'@tag':'Trap','@args':[normal(values['identity'])]}
        else:
            expected = None
        if expected is not None:
            require(same(backend,expected),'sealed terminator backend operands')
