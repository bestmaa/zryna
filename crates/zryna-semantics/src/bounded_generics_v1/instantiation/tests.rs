use super::*;
use crate::bounded_generics_v1::tests::body_fixtures::project;
use crate::bounded_generics_v1::{SemanticInput, resolve_declarations};
use zryna_source::NormalizedSourcePath;

type Inventory = (Vec<Vec<u8>>, Vec<Vec<u8>>, usize);

pub(super) fn check(files: &[(&str, &str)]) -> Result<Inventory, InstantiationFailure> {
    let input = project(files);
    let entry = input
        .sources
        .file_id(&NormalizedSourcePath::new("main.zry").expect("fixture path"))
        .expect("fixture entry");
    let declarations = resolve_declarations(
        SemanticInput::try_new(&input.syntax, &input.sources, entry)
            .expect("original source binding"),
    )
    .expect("original declarations");
    let bodies =
        super::super::body_types::check_body_types(&declarations).expect("original opaque bodies");
    let instances = discover(&bodies)?;
    assert!(std::ptr::eq(instances.bodies(), &raw const bodies));
    Ok((
        instances.type_keys().map(<[u8]>::to_vec).collect(),
        instances.function_keys().map(<[u8]>::to_vec).collect(),
        instances.edges().len(),
    ))
}

#[test]
fn identity_key_and_argument_order_match_fixed_oracles() {
    let (_,functions,edges)=check(&[("main.zry","function identity<T extends ZrynaValue>(x:T):T {return x;} function score():i32 {return identity<i32>(7);}")]).expect("closed identity");
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0], vec![0x40, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1]);
    assert_eq!(edges, 1);
}

#[test]
fn finite_supplied_box_and_same_key_indirection_are_distinct_from_expansion() {
    let (types,_,_)=check(&[("main.zry","interface Box<T extends ZrynaValue> extends ZrynaStruct {value:T;} function read(x:Box<Box<i32>>):i32 {return 0;}")]).expect("finite supplied tree");
    assert_eq!(types.iter().filter(|key| key[0] == 0x12).count(), 2);
    let (_,_,edges)=check(&[("main.zry","interface Node<T extends ZrynaValue> extends ZrynaStruct {next:Vec<Node<T>>;} function read(x:Node<i32>):i32 {return 0;}")]).expect("same-key finite indirection");
    assert_eq!(edges, 2);
    let failure=check(&[("main.zry","interface Nest<T extends ZrynaValue> extends ZrynaStruct {next:Vec<Nest<Vec<T>>>;} function read(x:Nest<i32>):i32 {return 0;}")]).expect_err("expanding generated declaration");
    let InstantiationFailure::Diagnostics(errors) = failure else {
        panic!("source diagnostic");
    };
    assert_eq!(errors[0].code(), "ZRYNA-M7003");
}

#[test]
fn unused_source_function_recursion_rejects_before_instances() {
    let failure = check(&[(
        "main.zry",
        "function recurse<T extends ZrynaValue>(x:T):T {return recurse<T>(x);}",
    )])
    .expect_err("unused source recursion");
    let InstantiationFailure::Diagnostics(errors) = failure else {
        panic!("source diagnostic");
    };
    assert_eq!(errors[0].code(), "ZRYNA-M7003");
}

#[test]
fn nested_option_depth_exact_and_first_extra() {
    for (depth, admitted) in [(64, true), (65, false)] {
        let ty = format!("{}i32{}", "Option<".repeat(depth), ">".repeat(depth));
        let source = format!("function read(x:{ty}):i32 {{return 0;}}");
        let result = check(&[("main.zry", &source)]);
        if admitted {
            let (types, _, _) = result.expect("exact depth");
            assert_eq!(types.iter().filter(|key| key[0] == 0x14).count(), 64);
            assert!(types.iter().any(|key| key.len() == 577));
        } else {
            let InstantiationFailure::Diagnostics(errors) = result.expect_err("first-extra depth")
            else {
                panic!("source budget");
            };
            assert_eq!(errors[0].code(), "ZRYNA-M7201");
        }
    }
}

#[test]
fn imported_alias_calls_deduplicate_original_instance_and_edge() {
    let (_,functions,edges)=check(&[("main.zry","import { identity as relay } from \"./values.zry\"; function score():i32 {relay<i32>(1);return relay<i32>(2);}"),("values.zry","export function identity<T extends ZrynaValue>(x:T):T {return x;}")]).expect("same original instance");
    assert_eq!(functions.len(), 1);
    assert_eq!(edges, 1);
    assert_eq!(&functions[0][1..5], &1u32.to_le_bytes());
}

#[test]
fn standard_enum_keys_are_reserved_and_sorted() {
    let (types,_,_)=check(&[("main.zry","function option():Option<i32> {return Option.some<i32>(7);} function result():Result<i32,bool> {return Result.err<i32,bool>(true);}")]).expect("standard enum keys");
    assert!(types.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(types.contains(&vec![0x14, 1, 0, 0, 0, 1, 0, 0, 0, 1]));
    assert!(types.contains(&vec![0x15, 2, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0]));
}

#[test]
fn nominal_borrow_fields_cannot_satisfy_zryna_value() {
    let error=check(&[("main.zry","interface Bad extends ZrynaStruct {loan:Borrow<i32>;} function identity<T extends ZrynaValue>(x:T):T {return x;} function read(x:Bad):Bad {return identity<Bad>(x);}")]).expect_err("nominal member is unstorable");
    let InstantiationFailure::Diagnostics(errors) = error else {
        panic!("bound diagnostic");
    };
    assert_eq!(errors[0].code(), "ZRYNA-M7001");
}

#[test]
fn two_ordered_arguments_remain_distinct_after_forwarding() {
    let (_, functions, edges) = check(&[("main.zry", "function first<T extends ZrynaValue,E extends ZrynaValue>(x:T,y:E):T {return x;} function relay<T extends ZrynaValue,E extends ZrynaValue>(x:T,y:E):T {return first<T,E>(x,y);} function number():i32 {return relay<i32,bool>(7,true);} function flag():bool {return relay<bool,i32>(true,7);}")]).expect("ordered substitutions");
    assert_eq!(functions.len(), 4);
    assert_eq!(edges, 4);
    let prefixes = functions.iter().map(|key| key[5]).collect::<Vec<_>>();
    assert_eq!(prefixes, [0, 0, 1, 1]);
    assert_eq!(&functions[0][9..], &[2, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 1]);
    assert_eq!(&functions[1][9..], &[2, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0]);
}

#[test]
fn diamond_imports_preserve_one_original_instance_and_replay() {
    let files = [
        (
            "main.zry",
            "import { left } from \"./a.zry\"; import { right } from \"./b.zry\"; function score():i32 {return left<i32>(7)+right<i32>(9);}",
        ),
        (
            "a.zry",
            "import { identity as core } from \"./values.zry\"; export function left<T extends ZrynaValue>(x:T):T {return core<T>(x);}",
        ),
        (
            "b.zry",
            "import { identity as core } from \"./values.zry\"; export function right<T extends ZrynaValue>(x:T):T {return core<T>(x);}",
        ),
        ("values.zry", "export function identity<T extends ZrynaValue>(x:T):T {return x;}"),
    ];
    let first = check(&files).expect("diamond closure");
    assert_eq!(first.1.len(), 3);
    assert_eq!(first.2, 4);
    assert_eq!(first.1.iter().filter(|key| key[1] == 3).count(), 1);
    assert_eq!(first, check(&files).expect("independent source authority replay"));
}

#[test]
fn borrowing_does_not_add_a_closed_key_application_level() {
    let ty = format!("{}i32{}", "Option<".repeat(64), ">".repeat(64));
    let source = format!("function read(x:Borrow<{ty}>):i32 {{return 0;}}");
    let (types, _, _) = check(&[("main.zry", &source)]).expect("borrow wrapper is not a key node");
    assert_eq!(types.iter().filter(|key| key[0] == 0x14).count(), 64);
}

#[test]
fn standard_payload_instances_have_their_own_dependency_edges() {
    let (types, functions, edges) =
        check(&[("main.zry", "function read(x:Option<Vec<Option<i32>>>):i32 {return 0;}")])
            .expect("closed standard payload");
    assert_eq!(types.iter().filter(|key| key[0] == 0x14).count(), 2);
    assert!(functions.is_empty());
    // The function root references both families; the outer family references its inner payload.
    assert_eq!(edges, 3);
}

#[test]
fn nominal_argument_rejection_uses_utf8_bytes_after_crlf() {
    let source = "function text():String {return \"é😀\";}\r\ninterface Bad extends ZrynaStruct {loan:Borrow<i32>;}\r\ninterface Box<T extends ZrynaValue> extends ZrynaStruct {value:T;}\r\nfunction read(x:Box<Bad>):i32 {return 0;}";
    let InstantiationFailure::Diagnostics(errors) =
        check(&[("main.zry", source)]).expect_err("non-storable nominal argument")
    else {
        panic!("source diagnostic");
    };
    assert_eq!(errors[0].code(), "ZRYNA-M7001");
    let span = errors[0].primary_span().expect("original argument span");
    let start =
        u32::try_from(source.rfind("Bad>").expect("argument text")).expect("fixture byte offset");
    assert_eq!((span.start(), span.end()), (start, start + 3));
}

#[test]
fn mutual_generated_expansion_uses_the_original_application_head() {
    let source = "interface A<T extends ZrynaValue> extends ZrynaStruct {next:Vec<B<T>>;} interface B<T extends ZrynaValue> extends ZrynaStruct {next:Vec<A<Vec<T>>>;} function read(x:A<i32>):i32 {return 0;}";
    let InstantiationFailure::Diagnostics(errors) =
        check(&[("main.zry", source)]).expect_err("mutual generated expansion")
    else {
        panic!("source diagnostic");
    };
    assert_eq!(errors[0].code(), "ZRYNA-M7003");
    let span = errors[0].primary_span().expect("generated application head");
    let start = u32::try_from(source.find("A<Vec<T>>").expect("generated head"))
        .expect("fixture byte offset");
    assert_eq!((span.start(), span.end()), (start, start + 1));
}

#[test]
fn invalid_supplied_nominal_is_rejected_before_declaration_expansion() {
    let source = "interface Nest<T extends ZrynaValue> extends ZrynaStruct {next:Vec<Nest<Vec<T>>>;} interface Bad extends ZrynaStruct {tree:Nest<i32>;loan:Borrow<i32>;} function identity<T extends ZrynaValue>(x:T):T {return x;} function read(x:Bad):Bad {return identity<Bad>(x);}";
    let InstantiationFailure::Diagnostics(errors) =
        check(&[("main.zry", source)]).expect_err("validate supplied arguments before expansion")
    else {
        panic!("source diagnostic");
    };
    assert_eq!(errors[0].code(), "ZRYNA-M7001");
    let span = errors[0].primary_span().expect("explicit argument span");
    let start =
        u32::try_from(source.rfind("Bad>").expect("argument text")).expect("fixture offset");
    assert_eq!((span.start(), span.end()), (start, start + 3));
}

#[test]
fn generated_diamonds_and_same_key_cycles_keep_separate_argument_paths() {
    let source = "interface A<T extends ZrynaValue> extends ZrynaStruct {left:Vec<B<T>>;right:Vec<C<T>>;} interface B<T extends ZrynaValue> extends ZrynaStruct {next:Vec<D<T>>;} interface C<T extends ZrynaValue> extends ZrynaStruct {next:Vec<D<T>>;} interface D<T extends ZrynaValue> extends ZrynaStruct {next:Vec<A<T>>;} function read(a:A<i32>,b:A<bool>):i32 {return 0;}";
    let (types, functions, edges) = check(&[("main.zry", source)]).expect("finite generated paths");
    assert_eq!(types.iter().filter(|key| key[0] == 0x12).count(), 8);
    assert!(functions.is_empty());
    assert_eq!(edges, 12);
}
