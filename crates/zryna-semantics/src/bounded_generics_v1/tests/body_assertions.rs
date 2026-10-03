//! Original-source type and diagnostic assertions shared by the body-type fixtures.

use super::super::{BodyTypeFailure, DeclarationContext, check_body_types};
use crate::bounded_generics_v1::tests::body_fixtures::{Project, project};
use crate::bounded_generics_v1::{SemanticInput, resolve_declarations};
use zryna_diagnostics::Diagnostic;
use zryna_source::NormalizedSourcePath;

pub(super) fn all_variants(source: &str) {
    use std::{collections::HashSet, mem::discriminant};
    let input = project(&[("main.zry", source)]);
    let context = declarations(&input);
    let functions = &context.syntax().files()[0].functions;
    let expressions = functions
        .iter()
        .flat_map(|function| &function.body.expressions)
        .map(|expression| discriminant(&expression.kind))
        .collect::<HashSet<_>>();
    let statements = functions
        .iter()
        .flat_map(|function| &function.body.statements)
        .map(|statement| discriminant(&statement.kind))
        .collect::<HashSet<_>>();
    assert_eq!(expressions.len(), 28);
    assert_eq!(statements.len(), 8);
    check_body_types(&context).expect("all authenticated body variants");
}

pub(super) fn legacy_constructors() {
    let declarations = "interface Pair extends ZrynaStruct {value:i32;} interface Ordered extends ZrynaStruct {first:i32;second:i32;} interface Choice extends ZrynaEnum {some:i32;none:ZrynaNone;}";
    for &(result, value, token, code, message, guidance) in LEGACY_CONSTRUCTORS {
        let source = format!("{declarations} function make():{result} {{return {value};}}");
        let errors = errors(&source);
        assert_eq!(errors.len(), 1, "{source}: {errors:?}");
        assert_eq!(errors[0].code(), code);
        assert_eq!(errors[0].message(), message);
        assert_eq!(errors[0].guidance, guidance);
        let span = errors[0].primary_span().expect("original constructor span");
        assert_eq!(&source[span.start() as usize..span.end() as usize], token);
    }
}

pub(super) fn pristine_replay() {
    use super::super::check_with_cache;
    let bad =
        project(&[("main.zry", "function bad<T extends ZrynaValue>(x:T):T {return clone(x);}")]);
    let good = project(&[("main.zry", super::IDENTITY)]);
    let bad_context = declarations(&bad);
    let good_context = declarations(&good);
    for capacity in [0, 1, 2, 256] {
        let BodyTypeFailure::Diagnostics(errors) = check_with_cache(&bad_context, capacity)
            .expect_err("source rejection before pristine replay")
        else {
            panic!("source rejection must retain diagnostics")
        };
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code(), "ZRYNA-M7002");
        check_with_cache(&good_context, capacity).expect("independent pristine replay");
    }
}

pub(super) fn nested_borrowed_arms(source: &str) {
    let input = project(&[("main.zry", source)]);
    let context = declarations(&input);
    let checked = check_body_types(&context).expect("nested original borrowed arms");
    let function = &checked.tables.functions[0];
    assert_eq!(function.arms.len(), 6);
    assert_eq!(function.environments.len(), 3);
    assert!(function.arms.iter().enumerate().all(|(index, arm)| {
        function.arms[..index].iter().all(|earlier| earlier.origin != arm.origin)
    }));
    assert!(function.arms.iter().all(|arm| arm.rank > 0 && arm.head.is_some()));
}

pub(super) fn borrowed_arms(source: &str, exclusive: bool, environments: usize) {
    use super::super::{
        equality,
        model::{Kind, Ty},
    };
    let input = project(&[("main.zry", source)]);
    let context = declarations(&input);
    let checked = check_body_types(&context).expect("borrowed match fixture");
    let function = &checked.tables.functions[0];
    assert_eq!(function.arms.len(), 2);
    assert_eq!(function.environments.len(), environments);
    assert_ne!(function.arms[0].origin, function.arms[1].origin);
    let access = if exclusive { Kind::BorrowMut } else { Kind::Borrow };
    assert!(
        function
            .arms
            .iter()
            .all(|arm| arm.rank > 0 && arm.head.expect("original arm head").kind == access)
    );
    let mut cache = equality::Cache::new(0).expect("fallible cache");
    assert!(
        equality::equal(
            &checked.tables,
            function.owner,
            Ty { origin: function.arms[0].origin, environment: 0 },
            Ty { origin: function.arms[1].origin, environment: 0 },
            &mut cache
        )
        .expect("exact borrowed shapes")
    );
}

pub(super) fn function(
    context: &DeclarationContext<'_>,
    index: usize,
) -> crate::bounded_generics_v1::DeclarationIdentity {
    context
        .modules()
        .next()
        .expect("original module")
        .functions()
        .nth(index)
        .expect("original function")
        .identity()
}

pub(super) fn parameter(
    context: &DeclarationContext<'_>,
    index: usize,
) -> crate::bounded_generics_v1::TypeParameterIdentity {
    context
        .declaration(function(context, index))
        .expect("original owner")
        .type_parameters()
        .next()
        .expect("original parameter")
        .identity()
}

pub(super) fn declarations(project: &Project) -> DeclarationContext<'_> {
    let entry = project
        .sources
        .file_id(&NormalizedSourcePath::new("main.zry").expect("original fixture invariant"))
        .expect("original fixture invariant");
    resolve_declarations(
        SemanticInput::try_new(&project.syntax, &project.sources, entry)
            .expect("original fixture invariant"),
    )
    .expect("original fixture invariant")
}

pub(super) fn passes(source: &str) {
    let input = project(&[("main.zry", source)]);
    check_body_types(&declarations(&input)).expect("complete source type checks");
}

pub(super) fn errors(source: &str) -> Vec<Diagnostic> {
    let input = project(&[("main.zry", source)]);
    let context = declarations(&input);
    let BodyTypeFailure::Diagnostics(errors) =
        check_body_types(&context).expect_err("no partial body context")
    else {
        panic!("source negative must have an owning diagnostic");
    };
    errors
}

pub(super) fn rejects(source: &str, code: &str, token: &str) {
    let errors = errors(source);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code(), code);
    let span = errors[0].primary_span().expect("original fixture invariant");
    assert_eq!(&source[span.start() as usize..span.end() as usize], token);
}

pub(super) fn has_error(source: &str, code: &str, token: &str) {
    assert!(errors(source).iter().any(|error| {
        let span = error.primary_span().expect("original fixture invariant");
        error.code() == code && &source[span.start() as usize..span.end() as usize] == token
    }));
}

const LEGACY_CONSTRUCTORS: &[(&str, &str, &str, &str, &str, &str)] = &[
    (
        "Pair",
        "Pair({value:true,other:1})",
        "other:1",
        "ZRYNA-M3005",
        "struct 'Pair' has no field 'other'",
        "initialize exactly the declared field set",
    ),
    (
        "Ordered",
        "Ordered({second:true})",
        "Ordered({second:true})",
        "ZRYNA-M3005",
        "field 'first' is not initialized",
        "initialize every declared field exactly once",
    ),
    (
        "Ordered",
        "Ordered({second:true,first:false})",
        "first:false",
        "ZRYNA-M3007",
        "struct field has a different exact aggregate type",
        "use a value with the exact declared type",
    ),
    (
        "Pair",
        "Pair({})",
        "Pair({})",
        "ZRYNA-M3005",
        "field 'value' is not initialized",
        "initialize every declared field exactly once",
    ),
    (
        "Pair",
        "Pair({other:1})",
        "other:1",
        "ZRYNA-M3005",
        "struct 'Pair' has no field 'other'",
        "initialize exactly the declared field set",
    ),
    (
        "Pair",
        "Pair({value:1,value:2})",
        "value:2",
        "ZRYNA-M3005",
        "field 'value' is initialized more than once",
        "initialize every declared field exactly once",
    ),
    (
        "Pair",
        "Pair({value:true})",
        "value:true",
        "ZRYNA-M3007",
        "struct field has a different exact aggregate type",
        "use a value with the exact declared type",
    ),
    (
        "Choice",
        "Choice.missing()",
        "missing",
        "ZRYNA-M3005",
        "enum 'Choice' has no variant 'missing'",
        "use one exact declared variant",
    ),
    (
        "Choice",
        "Choice.some()",
        "Choice.some()",
        "ZRYNA-M3005",
        "enum payload presence does not match the declared variant",
        "supply exactly one payload only for a payload variant",
    ),
    (
        "Choice",
        "Choice.none(1)",
        "Choice.none(1)",
        "ZRYNA-M3005",
        "enum payload presence does not match the declared variant",
        "supply exactly one payload only for a payload variant",
    ),
    (
        "Choice",
        "Choice.some(true)",
        "true",
        "ZRYNA-M3007",
        "enum payload has a different exact aggregate type",
        "use a value with the exact declared type",
    ),
    (
        "Choice",
        "Choice({})",
        "Choice({})",
        "ZRYNA-M3005",
        "struct construction names an enum",
        "use enum variant construction for an enum",
    ),
    (
        "Pair",
        "Pair.some(1)",
        "Pair.some(1)",
        "ZRYNA-M3005",
        "enum construction names a struct",
        "construct a declared enum variant",
    ),
    (
        "FixedArray<i32,2>",
        "FixedArray<i32,2>([1])",
        "FixedArray<i32,2>([1])",
        "ZRYNA-M3005",
        "fixed-array constructor has 1 elements but its type requires 2",
        "provide exactly the fixed-array length",
    ),
];
