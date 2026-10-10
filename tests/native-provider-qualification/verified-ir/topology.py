"""Link the complete sealed Debug structure to its getter topology and source authority."""
from debug_reader import argument, fields, parse
import operations

def require(value, message):
    if not value:
        raise ValueError(message)

def exact_fields(value, tag, names):
    result = fields(value, tag)
    require(set(result) == set(names.split()), 'complete sealed '+tag+' fields')
    return result

def span(value):
    value = fields(value,'Span')
    return [fields(value['file'],'FileId')['index'],value['start'],value['end']]

def index(value):
    result = argument(value) if '@args' in value else fields(value)['index']
    require(type(result) is int and result >= 0,'exact sealed/getter identity index')
    return result

def list_equal(raw, observed, label):
    require(type(raw) is list and type(observed) is list and len(raw) == len(observed), 'complete sealed/getter '+label+' topology')
    return zip(raw,observed)

def check_raw_spans(node, sources, source_map_ids):
    if type(node) is dict:
        if node.get('@tag') == 'FileId':
            value = exact_fields(node,'FileId','source_map_id index')
            require(type(value['index']) is int and 0 <= value['index'] < len(sources),'sealed source-bound FileId')
            source_map_ids.add(value['source_map_id'])
        if node.get('@tag') == 'Span':
            value = span(node)
            require(all(type(n) is int for n in value) and 0 <= value[0] < len(sources), 'sealed span types')
            require(0 <= value[1] <= value[2] <= sources[value[0]]['bytes'],'sealed original source byte span')
        for value in node.values():
            check_raw_spans(value,sources,source_map_ids)
    elif type(node) is list:
        for value in node:
            check_raw_spans(value,sources,source_map_ids)

def check_parameters(raw, projected, profile):
    for source, getter in list_equal(raw,projected,'parameter'):
        source = fields(source,'ValueDefinition')
        require(index(source['id']) == index(parse(getter['id'])),'sealed parameter identity')
        require(span(source['span']) == getter['span'],'sealed parameter span')
        if profile == 'm2':
            require(source['ty'] == parse(getter['type']),'sealed parameter type')
        else:
            require(index(source['ty']) == index(parse(getter['type'])),'sealed parameter type identity')

def check(profile, raw_debug, getters, sources, entrypoint):
    raw = parse(raw_debug)
    names = 'program abi' if profile == 'm1' else ('program abi abi_indices' if profile == 'm2' else 'program identity linear32 linux_x86_64 abi abi_indices borrow_indices')
    root = exact_fields(raw,'VerifiedProgram',names)
    ids = set()
    check_raw_spans(raw,sources,ids)
    require(len(ids) == 1,'one nominal original SourceMap identity')
    if profile == 'm1':
        program = exact_fields(root['program'],'Program','functions')
        for source,getter in list_equal(program['functions'],getters['functions'],'M1 function'):
            source = exact_fields(source,'Function','name parameters return_type expressions body')
            require(source['name'] == argument(parse(getter['export'])),'sealed M1 export name')
            for name, field in (('parameters','parameters'),('return_type','result'),('expressions','expressions'),('body','body')):
                require(operations.same(source[name],parse(getter[field])),'sealed M1 '+name)
    else:
        program = exact_fields(root['program'],'Program','entry_module modules' + (' authorities' if profile == 'm3' else ''))
        require(index(program['entry_module']) == next(s['file_id'] for s in sources if s['path'] == entrypoint),'sealed explicit entry module')
        if profile == 'm2':
            require(index(program['entry_module']) == index(parse(getters['entry_module'])),'entry module getter')
        else:
            for name,field in (('identity','identity'),('linear32','linear32_layouts'),('linux_x86_64','linux_x86_64_layouts'),('abi','abi')):
                require(operations.same(root[name],parse(getters[field])),'complete sealed/getter '+name)
            authorities = fields(program['authorities'],'AuthorityClaims')
            require(authorities['runtime'] == parse(getters['runtime_contract']),'sealed runtime contract getter')
            layout_records = fields(root['linear32'],'VerifiedLayouts')['records']
            identity = fields(root['identity'],'ProgramIdentity')
            require(operations.same(identity['source_map'],parse(getters['source_map_identity'])),'sealed source map identity')
            require(operations.same(identity['universe'],parse(getters['type_universe_identity'])),'sealed universe identity')
            require(argument(identity['source_map']) == next(iter(ids)),'source map authority binding')
        for source,getter in list_equal(program['modules'],getters['modules'],'module'):
            source = exact_fields(source,'Module','id source_file functions' + (' data_declarations' if profile == 'm3' else ''))
            require(index(source['id']) == index(parse(getter['id'])),'sealed module identity')
            require(type(getter['source_file']) is int and fields(source['source_file'],'FileId')['index'] == getter['source_file'],'sealed module source authority')
            if profile == 'm3':
                require(type(getter['data_declarations']) is int and source['data_declarations'] == getter['data_declarations'],'sealed data declaration count')
            for function,projected in list_equal(source['functions'],getter['functions'],'all including private function'):
                function = exact_fields(function,'Function','id entry_export span parameters result blocks' + (' borrow_parameters places cleanup_plans' if profile == 'm3' else ''))
                raw_id = fields(function['id'],'FunctionId')
                getter_id = fields(parse(projected['id']),'FunctionIdentity')
                raw_module = index(raw_id['module'])
                getter_module = index(getter_id['module']) if profile == 'm2' else getter_id['module']
                require(type(getter_module) is int and getter_module >= 0 and type(getter_id['declaration']) is int and getter_id['declaration'] >= 0,'exact function module/declaration indices')
                require(raw_module == getter_module and raw_id['declaration'] == getter_id['declaration'],'sealed function identity')
                require((function['entry_export'].get('@tag') == 'None') == (parse(projected['public_export']).get('@tag') == 'None'),'complete public/private export status')
                check_parameters(function['parameters'],projected['parameters'],profile)
                if profile == 'm2':
                    require(function['result'] == parse(projected['result']) and span(function['span']) == projected['span'],'sealed result and function span')
                else:
                    require(index(function['result']) == index(parse(projected['result_type'])),'sealed result type identity')
                    for parameter,value in list_equal(function['borrow_parameters'],projected['borrow_parameters'],'borrow parameter'):
                        parameter = fields(parameter,'BorrowParameter')
                        require(index(parameter['id']) == index(parse(value['id'])) and span(parameter['span']) == value['span'],'sealed borrow identity and span')
                        require(parameter['access'] == parse(value['access']) and index(parameter['referent']) == index(parse(value['referent'])),'sealed borrow access and referent')
                    for place,value in list_equal(function['places'],projected['places'],'place'):
                        place = fields(place,'Place')
                        require(index(place['id']) == index(parse(value['id'])) and span(place['span']) == value['span'],'sealed place identity and span')
                        require(index(place['ty']) == index(parse(value['type'])),'sealed place type identity')
                        require(operations.same(operations.normal(place['kind']),operations.normal(parse(value['kind']))),'sealed complete place kind/projection')
                        layout = fields(layout_records[index(place['ty'])],'LayoutRecord')
                        require(type(value['is_copy']) is bool and value['is_copy'] == (layout['drop_kind'] == 0),'sealed place Copy authority')
                    for plan,value in list_equal(function['cleanup_plans'],projected['cleanup_plans'],'cleanup plan'):
                        plan = fields(plan,'CleanupPlan')
                        require(index(plan['id']) == index(parse(value['id'])) and span(plan['span']) == value['span'],'sealed cleanup identity and span')
                        require([index(argument(action)) for action in plan['actions']] == [index(action) for action in parse(value['actions'])],'sealed cleanup action identity/order')
                for block,value in list_equal(function['blocks'],projected['blocks'],'block'):
                    block = exact_fields(block,'Block','id parameters instructions terminators')
                    require(index(block['id']) == index(parse(value['id'])),'sealed block identity')
                    check_parameters(block['parameters'],value['parameters'],profile)
                    for raw_instruction,getter_instruction in list_equal(block['instructions'],value['instructions'],'instruction'):
                        operations.instruction(profile,raw_instruction,getter_instruction,span)
                    require(len(block['terminators']) == 1,'exactly one sealed terminator')
                    operations.terminator(profile,block['terminators'][0],value['terminator'],span)
    require(operations.same(root['abi'],parse(getters['abi'])),'complete sealed ABI getter')
