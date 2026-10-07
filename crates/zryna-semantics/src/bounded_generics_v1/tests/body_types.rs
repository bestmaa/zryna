use super::*;
use crate::bounded_generics_v1::tests::body_fixtures::project;

#[path = "body_assertions.rs"]
mod assertions;
use assertions::{declarations, errors, function, has_error, parameter, passes, rejects};

const IDENTITY: &str = "function identity<T extends ZrynaValue>(value: T): T { return value; }";
const BOX: &str = "interface Box<T extends ZrynaValue> extends ZrynaStruct { value: T; }";

#[test]
fn exact_declaration_context_retained() {
    let input = project(&[("main.zry", IDENTITY)]);
    let context = declarations(&input);
    let checked = check_body_types(&context).expect("original fixture invariant");
    assert!(std::ptr::eq(checked.declarations(), &raw const context));
    let foreign = project(&[("main.zry", IDENTITY)]);
    let foreign_owner = function(&declarations(&foreign), 0);
    assert_eq!(checked.expression_count(foreign_owner), None);
}

#[test]
fn owning_parameter_terms_are_disjoint() {
    let source = format!(
        "{IDENTITY} function relay<U extends ZrynaValue>(value: U): U {{ return identity<U>(value); }}"
    );
    let input = project(&[("main.zry", &source)]);
    let context = declarations(&input);
    let checked = check_body_types(&context).expect("original fixture invariant");
    let first = parameter(&context, 0);
    let second = parameter(&context, 1);
    assert_ne!(first, second);
    let result = checked.expression_type(function(&context, 1), 1).expect("original expression");
    assert_eq!(result.shape(), TypeShape::Parameter(second));
    let input = crate::bounded_generics_v1::tests::body_fixtures::many_owners();
    let context = declarations(&input);
    let checked = check_body_types(&context).expect("source-derived high owner count");
    let owners = context
        .modules()
        .flat_map(|module| module.data_declarations().chain(module.functions()))
        .flat_map(|declaration| {
            declaration
                .type_parameters()
                .map(|parameter| (parameter.identity().declaration(), parameter.identity().index()))
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(owners.len(), 65_528);
    assert_eq!(checked.storage().predicate_rows, 65_537);
}

#[test]
fn unused_identity_template_typechecked() {
    passes(IDENTITY);
}

#[test]
fn unused_invalid_clone_template_rejected() {
    rejects(
        "function broken<T extends ZrynaValue>(value:T):T { return clone(value); }",
        "ZRYNA-M7002",
        "clone",
    );
}

#[test]
fn concrete_i32_use_does_not_repair_clone() {
    rejects(
        "function broken<T extends ZrynaValue>(value:T):T { return clone(value); } function score():i32 { return broken<i32>(1); }",
        "ZRYNA-M7002",
        "clone",
    );
}

#[test]
fn two_parameter_template_keeps_order() {
    passes(
        "function second<T extends ZrynaValue,E extends ZrynaValue>(a:T,b:E):E { return b; } function relay<A extends ZrynaValue,B extends ZrynaValue>(a:A,b:B):B { return second<A,B>(a,b); }",
    );
    rejects(
        "function second<T extends ZrynaValue,E extends ZrynaValue>(a:T,b:E):T { return b; }",
        "ZRYNA-M7006",
        "b",
    );
}

#[test]
fn imported_template_substitution_uses_original_owner() {
    let input = project(&[
        (
            "main.zry",
            "import { identity as relay, Box as Parcel } from \"./values.zry\"; function wrap<T extends ZrynaValue>(value:T):T { const p:Parcel<T> = Parcel<T>({value:value}); return relay<T>(p.value); }",
        ),
        (
            "values.zry",
            "export interface Box<T extends ZrynaValue> extends ZrynaStruct { value:T; } export function identity<T extends ZrynaValue>(value:T):T {return value;}",
        ),
    ]);
    let context = declarations(&input);
    let checked = check_body_types(&context).expect("original fixture invariant");
    let original = context
        .modules()
        .nth(1)
        .expect("original fixture invariant")
        .functions()
        .next()
        .expect("original fixture invariant")
        .identity();
    assert!(
        checked.tables.functions[0]
            .environments
            .iter()
            .any(|environment| environment.owner == original)
    );
}

#[test]
fn local_binding_initializers_and_shadowing() {
    passes("function score(value:i32):i32 { { const value:i32 = value; value; } return value; }");
    rejects(
        "function score():i32 { { const hidden:i32 = 1; } return hidden; }",
        "ZRYNA-M3002",
        "hidden",
    );
}

#[test]
fn branch_loop_and_upgrade_lexical_types() {
    passes(
        "function score(flag:bool, weak:Weak<String>):i32 { let value:i32 = 0; if(flag) { value = 1; } else { value = 2; } while(flag) { value = value + 1; } upgradeWeak(weak,(strong)=>{ clone(strong); },()=>{ value = 3; }); return value; }",
    );
    rejects(
        "function score(weak:Weak<String>):i32 { upgradeWeak(weak,(strong)=>{ clone(strong); },()=>{ strong; }); return 0; }",
        "ZRYNA-M3002",
        "strong",
    );
}

#[test]
fn opaque_scalar_operations_rejected() {
    for operator in ["+", "-", "*", "===", "!==", "<", "<=", ">", ">="] {
        rejects(
            &format!(
                "function bad<T extends ZrynaValue>(a:T,b:T):T {{ a {operator} b; return a; }}"
            ),
            "ZRYNA-M7002",
            operator,
        );
    }
    rejects("function bad<T extends ZrynaValue>(a:T):T { -a; return a; }", "ZRYNA-M7002", "-");
}

#[test]
fn opaque_projection_index_and_callable_rejected() {
    for (body, token) in [("a.value;", "."), ("a[0];", "["), ("a();", "a")] {
        rejects(
            &format!("function bad<T extends ZrynaValue>(a:T):T {{ {body} return a; }}"),
            "ZRYNA-M7002",
            token,
        );
    }
}

#[test]
fn higher_kinded_application_is_resolved_before_code() {
    rejects(
        "function bad<F extends ZrynaValue>(value:F<i32>):i32 { return 0; }",
        "ZRYNA-M7002",
        "F<i32>",
    );
    rejects(
        &format!("{IDENTITY} function score():i32 {{ return identity<Unknown>(1); }}"),
        "ZRYNA-M7001",
        "Unknown",
    );
}

#[test]
fn all_payload_capabilities_are_required() {
    for value in ["Option.none<T>()", "Vec<T>([])", "Box<T>({value:value})", "Choice.none<T>()"] {
        let source = format!(
            "{BOX} interface Choice<T extends ZrynaValue> extends ZrynaEnum {{ none:ZrynaNone; some:T; }} function bad<T extends ZrynaValue>(value:T):T {{ clone({value}); return value; }}"
        );
        rejects(&source, "ZRYNA-M7002", "clone");
    }
}

#[test]
fn shared_weak_handle_clone_does_not_require_payload_clone() {
    passes(
        "function wrap<T extends ZrynaValue>(value:T):Weak<T> { return clone(downgrade(clone(shared(value)))); }",
    );
    rejects(
        "function bad<T extends ZrynaValue>(value:T):T { clone(borrow(value)); return value; }",
        "ZRYNA-M3008",
        "clone",
    );
}

#[test]
fn symbolic_moves_stores_borrows_passes_returns_typecheck() {
    passes(&format!(
        "{IDENTITY} {BOX} function relay<T extends ZrynaValue>(value:T):T {{ const stored:Box<T> = Box<T>({{value:value}}); borrow(stored); borrowMut(stored); return identity<T>(stored.value); }}"
    ));
}

#[test]
fn ownership_misuse_remains_an_unresolved_obligation() {
    let source = format!(
        "{IDENTITY} function reuse<T extends ZrynaValue>(value:T):T {{ identity<T>(value); return value; }}"
    );
    let input = project(&[("main.zry", &source)]);
    let context = declarations(&input);
    let checked = check_body_types(&context).expect("original fixture invariant");
    let owner = context
        .modules()
        .next()
        .expect("original fixture invariant")
        .functions()
        .nth(1)
        .expect("original fixture invariant")
        .identity();
    assert_eq!(checked.expression_count(owner), Some(3));
    let uses = resources::raw_function(&context, owner)
        .body
        .expressions
        .iter()
        .filter(|expression| {
            matches!(&expression.kind,
                zryna_syntax::v5::RawExpressionKind::Reference { name } if name.text == "value")
        })
        .count();
    assert_eq!(uses, 2);
}

#[test]
fn opaque_barrier_precedes_all_argument_candidates() {
    let main = format!(
        "import {{bad,other}} from \"./late.zry\"; {IDENTITY} function score():i32 {{ {} return 0; }}",
        "identity(1);".repeat(256)
    );
    let input = project(&[
        ("main.zry", &main),
        (
            "late.zry",
            "export function bad<T extends ZrynaValue>(x:T):T {return clone(x);} export function other<U extends ZrynaValue>(x:U):U {return clone(x);}",
        ),
    ]);
    let context = declarations(&input);
    let BodyTypeFailure::Diagnostics(errors) =
        check_body_types(&context).expect_err("source rejection")
    else {
        panic!("diagnostic lane")
    };
    assert_eq!(errors.len(), 2);
    assert!(errors.iter().all(|error| error.code() == "ZRYNA-M7002"));
}

#[test]
fn invalid_type_hole_never_becomes_a_success_type() {
    rejects(
        "function bad<T extends ZrynaValue>(x:Option<i32>,t:T):T {return clone(match(x,{\"Option.none\":()=>t,\"Option.some\":(v)=>v}));}",
        "ZRYNA-M7006",
        "v",
    );
    rejects(
        &format!(
            "{IDENTITY} function bad<T extends ZrynaValue>(x:T):T {{ return clone(match(Option.some(x), {{\"Option.none\":()=>x,\"Option.some\":(v)=>x}})); }}"
        ),
        "ZRYNA-M7001",
        "Option",
    );
    rejects(
        &format!(
            "{IDENTITY} function bad<T extends ZrynaValue>(x:T):T {{ identity(1); return clone(x); }}"
        ),
        "ZRYNA-M7002",
        "clone",
    );
}

#[test]
fn explicit_argument_presence_count_and_categories() {
    passes(&format!("{IDENTITY} function score():i32 {{ return identity<i32>(1); }}"));
    for arguments in ["", "<i32,bool>", "<unit>", "<Borrow<i32>>", "<Vec<Borrow<i32>>>"] {
        let errors = errors(&format!(
            "{IDENTITY} function score():i32 {{ return identity{arguments}(1); }}"
        ));
        assert!(errors.iter().all(|error| error.code() == "ZRYNA-M7001"), "{errors:?}");
    }
}

#[test]
fn generic_function_value_requires_arguments() {
    rejects(
        &format!("{IDENTITY} function score():i32 {{ identity; return 0; }}"),
        "ZRYNA-M7001",
        "identity",
    );
    passes("function score():i32 { return 0; } function other():i32 { score; return score(); }");
}

#[test]
fn nominal_identity_and_argument_order_are_exact() {
    rejects(
        &format!(
            "{BOX} interface Other<T extends ZrynaValue> extends ZrynaStruct {{value:T;}} function bad<T extends ZrynaValue>(a:Box<T>):Other<T> {{return a;}}"
        ),
        "ZRYNA-M7006",
        "a",
    );
    rejects(
        "function bad<T extends ZrynaValue,E extends ZrynaValue>(a:Result<T,E>):Result<E,T> {return a;}",
        "ZRYNA-M7006",
        "a",
    );
}

#[test]
fn generic_struct_fields_use_original_declaration_order() {
    passes(
        "interface Pair<T extends ZrynaValue,E extends ZrynaValue> extends ZrynaStruct {first:T;second:E;} function make<T extends ZrynaValue,E extends ZrynaValue>(a:T,b:E):Pair<T,E> {return Pair<T,E>({second:b,first:a});}",
    );
    for (fields, token) in
        [("value:true", "true"), ("other:1", "other"), ("value:1,value:2", "value"), ("", "Box")]
    {
        let source = format!("{BOX} function score():Box<i32> {{return Box<i32>({{{fields}}});}}");
        let errors = errors(&source);
        assert!(errors.iter().all(|error| error.code() == "ZRYNA-M7006"));
        assert!(errors.iter().any(|error| {
            let span = error.primary_span().expect("original fixture invariant");
            &source[span.start() as usize..span.end() as usize] == token
        }));
    }
}

#[test]
fn standard_and_user_enum_signatures_are_exact() {
    passes(
        "interface Choice<T extends ZrynaValue> extends ZrynaEnum {none:ZrynaNone;some:T;} function make(x:i32):Choice<i32> {Option.none<i32>();Option.some<i32>(x);Result.ok<i32,String>(x);Result.err<i32,String>(\"x\");return Choice.some<i32>(x);}",
    );
    rejects(
        "function score():Option<i32> {return Option.unknown<i32>();}",
        "ZRYNA-M7004",
        "unknown",
    );
    rejects("function score():Option<i32> {return Option.some<i32>(true);}", "ZRYNA-M7006", "true");
    rejects("function score():Option<i32> {return Option.none<i32>(1);}", "ZRYNA-M7006", "none");
}

#[test]
fn match_family_coverage_bindings_and_results() {
    passes(
        "function score(x:Option<i32>):i32 {return match(x,{\"Option.some\":(v)=>v,\"Option.none\":()=>0});}",
    );
    for (arms, code, token) in [
        ("\"Option.none\":()=>0", "ZRYNA-M7004", "match"),
        (
            "\"Option.none\":()=>0,\"Option.none\":()=>0,\"Option.some\":(v)=>v",
            "ZRYNA-M7004",
            "none",
        ),
        ("\"Result.ok\":(v)=>v,\"Option.none\":()=>0", "ZRYNA-M7004", "Result"),
        ("\"Option.none\":()=>true,\"Option.some\":(v)=>v", "ZRYNA-M7006", "v"),
    ] {
        has_error(
            &format!("function score(x:Option<i32>):i32 {{return match(x,{{{arms}}});}}"),
            code,
            token,
        );
    }
}

#[test]
fn borrowed_match_payload_types_remain_borrowed() {
    for access in ["Borrow", "BorrowMut"] {
        for (family, left, right, environments) in
            [("Result<T,T>", "ok", "err", 0), ("Choice<T>", "a", "b", 1)]
        {
            let name = family.split('<').next().expect("family spelling");
            let source = format!(
                "interface Choice<T extends ZrynaValue> extends ZrynaEnum {{a:T;b:T;}} function select<T extends ZrynaValue>(x:{access}<{family}>):{access}<T> {{return match(x,{{\"{name}.{left}\":(a)=>a,\"{name}.{right}\":(b)=>b}});}}"
            );
            assertions::borrowed_arms(&source, access == "BorrowMut", environments);
        }
    }
    assertions::nested_borrowed_arms(
        "interface Choice<T extends ZrynaValue> extends ZrynaEnum {a:T;b:T;} function nested<T extends ZrynaValue>(x:Borrow<Choice<Choice<T>>>):Borrow<T> {return match(x,{\"Choice.a\":(a)=>match(a,{\"Choice.a\":(v)=>v,\"Choice.b\":(v)=>v}),\"Choice.b\":(b)=>match(b,{\"Choice.a\":(v)=>v,\"Choice.b\":(v)=>v})});}",
    );
    rejects(
        "function bad<T extends ZrynaValue>(x:Borrow<Option<T>>):i32 {return match(x,{\"Option.none\":()=>0,\"Option.some\":(v)=>clone(v)});}",
        "ZRYNA-M3008",
        "clone",
    );
}

#[test]
fn nongeneric_type_diagnostics_keep_existing_meaning() {
    assertions::legacy_constructors();
    rejects("function score():i32 {return missing;}", "ZRYNA-M3002", "missing");
    rejects("function score():i32 {return true;}", "ZRYNA-M3007", "true");
    rejects("function score():i32 {return 2147483648;}", "ZRYNA-M3008", "2147483648");
    rejects("function score():i32 { \"x\" === 1; return 0; }", "ZRYNA-M3007", "===");
    let errors = errors("function score(x:FixedArray<Foo,2>):i32 {return 0;}");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "ZRYNA-M3002");
    assert_eq!(errors[0].message(), "type 'Foo' does not name a module-local aggregate");
}

#[test]
fn source_order_and_terminal_diagnostics() {
    let source = format!("function score():i32 {{{} return 0;}}", "missing;".repeat(260));
    let errors = errors(&source);
    assert_eq!(errors.len(), 256);
    assert_eq!(errors[255].code(), "ZRYNA-M7201");
    assert!(
        errors.windows(2).all(|pair| pair[0]
            .primary_span()
            .expect("original fixture invariant")
            .start()
            < pair[1].primary_span().expect("original fixture invariant").start())
    );
}

#[test]
fn derived_storage_cache_eviction_and_pristine_replay() {
    assert_eq!(
        resources::comparison_domain(262_144, 65_536, 16_384, 4096, 16_384)
            .expect("original fixture invariant"),
        (7_466_307_588, 55_745_748_998_626_377_744, 5_316_866)
    );
    assert!(matches!(
        resources::reserve::<u64>(usize::MAX),
        Err(BodyTypeFailure::AllocationFailure)
    ));
    let input = project(&[(
        "main.zry",
        &format!(
            "{IDENTITY} function relay<T extends ZrynaValue>(x:Result<T,T>):Result<T,T> {{ return identity<Result<T,T>>(x); }}"
        ),
    )]);
    let context = declarations(&input);
    for capacity in [0, 1, 2, 256] {
        let checked = check_with_cache(&context, capacity).expect("original fixture invariant");
        assert!(checked.storage().table_capacity_bytes > 0);
        assert_eq!(checked.storage().environments, 1);
    }
    let chain = ".next".repeat(32);
    let source = format!(
        "interface Link<T extends ZrynaValue> extends ZrynaStruct {{next:Link<Result<T,T>>;}} function compare(a:Link<i32>,b:Link<i32>):i32 {{let value=a{chain}; value=b{chain}; value=b{chain}; return 0;}}"
    );
    let input = project(&[("main.zry", &source)]);
    let context = declarations(&input);
    for capacity in [0, 1, 2, 256] {
        let checked = check_with_cache(&context, capacity).expect("repeated substitution DAG");
        eprintln!("cache={capacity}, storage={:?}", checked.storage());
    }
    assertions::pristine_replay();
}

#[test]
fn all_v5_variants_have_semantic_rules() {
    assertions::all_variants(
        "interface Pair extends ZrynaStruct {value:i32;} interface Choice extends ZrynaEnum {some:i32;none:ZrynaNone;} function helper(x:i32):i32 {return x;} function score(flag:bool,owner:String,values:Vec<i32>,weak:Weak<String>):i32 {let n:i32 = 1; {const x:i32 = 2;} if(flag){n=2;}else{n=3;}while(flag){n=n+1;}true;false;\"s\";-n;n+n;n-n;n*n;n===1;n!==1;n<1;n<=1;n>1;n>=1;helper(n);clone(owner);shared(owner);downgrade(shared(owner));borrow(owner);borrowMut(values);push(values,1);Pair({value:n});Choice.some(n);Vec<i32>([1]);FixedArray<i32,1>([1]);values[0];upgradeWeak(weak,(strong)=>{clone(strong);},()=>{n=0;});match(Choice.some(n),{\"Choice.some\":(v)=>v,\"Choice.none\":()=>0});return Pair({value:n}).value;}",
    );
    let source = "function score():i32 {return unknown();}";
    rejects(source, "ZRYNA-M3002", "unknown");
}

#[test]
fn no_instance_or_ownership_certificate_is_created() {
    passes(
        "interface Nest<T extends ZrynaValue> extends ZrynaStruct {next:Vec<Nest<Vec<T>>>;} function recurse<T extends ZrynaValue>(x:T):T {return recurse<T>(x);}",
    );
}

#[test]
fn recursive_indirection_clone_keeps_greatest_fixed_point() {
    passes(
        "interface Node<T extends ZrynaValue> extends ZrynaStruct {next:Vec<Node<T>>;value:T;} function copy(x:Node<i32>):Node<i32> {return clone(x);}",
    );
    rejects(
        "interface Node<T extends ZrynaValue> extends ZrynaStruct {next:Vec<Node<T>>;value:T;} function bad<T extends ZrynaValue>(x:Node<T>):Node<T> {return clone(x);}",
        "ZRYNA-M7002",
        "clone",
    );
    rejects(
        "interface Node<T extends ZrynaValue> extends ZrynaStruct {next:Vec<Node<T>>;loan:Borrow<i32>;} function bad(x:Node<i32>):Node<i32> {return clone(x);}",
        "ZRYNA-M3008",
        "clone",
    );
}
