use super::super::{DeclarationKind, SemanticInput, resolve_declarations};
use super::fixtures::{self, Project};
use super::{context, source_text};
use zryna_diagnostics::Severity;
use zryna_syntax::v4::{RawDiagnosticLocation, RawProviderDiagnostic};
use zryna_syntax::v5::verify_snapshot;

#[test]
fn exact_v5_source_and_entry_retained() {
    let project = fixtures::reference();
    let resolved = context(&project, "main.zry").expect("original declaration context");
    assert!(std::ptr::eq(resolved.syntax(), &raw const project.syntax));
    assert!(std::ptr::eq(resolved.sources(), &raw const project.sources));
    assert_eq!(resolved.entry().index(), 0);
    let declarations = resolved
        .modules()
        .flat_map(|module| module.data_declarations().chain(module.functions()))
        .map(|declaration| declaration.name().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(declarations, ["score", "Box", "Choice", "identity", "select"]);
}

#[test]
fn independent_equal_text_source_map_rejected() {
    let first = fixtures::scalar();
    let second = fixtures::scalar();
    assert!(
        SemanticInput::try_new(
            &first.syntax,
            &second.sources,
            second.sources.verify_file_id(0).expect("second entry")
        )
        .is_none()
    );
    let first_context = context(&first, "main.zry").expect("first context");
    let second_context = context(&second, "main.zry").expect("second context");
    let identity = second_context
        .modules()
        .next()
        .expect("module")
        .functions()
        .next()
        .expect("function")
        .identity();
    assert!(first_context.declaration(identity).is_none());
}

#[test]
fn cloned_source_map_identity_accepted() {
    let project = fixtures::scalar();
    let cloned = project.sources.clone();
    let input = SemanticInput::try_new(
        &project.syntax,
        &cloned,
        cloned.verify_file_id(0).expect("cloned entry"),
    )
    .expect("retained issuing identity");
    let resolved = resolve_declarations(input).expect("original immutable clone");
    assert!(std::ptr::eq(resolved.sources(), &raw const cloned));
    assert_eq!(resolved.modules().next().expect("module").functions().count(), 1);
}

#[test]
fn foreign_entry_file_id_rejected() {
    let first = fixtures::scalar();
    let second = fixtures::scalar();
    let foreign = second.sources.verify_file_id(0).expect("foreign entry");
    assert_eq!(foreign.index(), first.sources.verify_file_id(0).expect("entry").index());
    assert!(SemanticInput::try_new(&first.syntax, &first.sources, foreign).is_none());
}

#[test]
fn empty_snapshot_has_no_entry_context() {
    let empty = fixtures::project(&[], |_, _| panic!("empty source map has no modules"));
    let other = fixtures::scalar();
    assert!(empty.syntax.files().is_empty());
    assert!(
        SemanticInput::try_new(
            &empty.syntax,
            &empty.sources,
            other.sources.verify_file_id(0).expect("foreign entry")
        )
        .is_none()
    );
}

fn advisory(severity: Severity) -> Project {
    let (sources, mut raw) =
        fixtures::raw_project(&["main.zry"], |_, unit| unit.function("score", &[], true));
    raw.diagnostics.push(RawProviderDiagnostic {
        code: "provider-message".into(),
        severity,
        location: RawDiagnosticLocation::Source {
            span: zryna_source::UntrustedSpan { file: 0, start: 0, end: 6 },
        },
        message: "this advisory does not assign declaration identity".into(),
        guidance: "inspect original source".into(),
    });
    let syntax = verify_snapshot(raw, &sources).expect("complete source with located advisory");
    Project { sources, syntax }
}

#[test]
fn provider_error_prevents_semantic_input() {
    let project = advisory(Severity::Error);
    assert!(project.syntax.is_bound_to(&project.sources));
    assert!(
        SemanticInput::try_new(
            &project.syntax,
            &project.sources,
            project.sources.verify_file_id(0).expect("entry")
        )
        .is_none()
    );
}

#[test]
fn provider_warning_retained_without_semantic_authority() {
    let project = advisory(Severity::Warning);
    let resolved = context(&project, "main.zry").expect("warning does not supply bindings");
    assert_eq!(resolved.syntax().advisories(), project.syntax.advisories());
    let function = resolved.modules().next().expect("module").functions().next().expect("function");
    assert_eq!(function.name(), "score");
    assert_eq!(source_text(&project, function.name_span()), "score");
}

#[test]
fn module_ids_follow_source_map_path_order() {
    let project = fixtures::project(&["z.zry", "a.zry"], |path, unit| {
        if path == "a.zry" {
            unit.import("./z.zry", &[("score", "importedScore")]);
        }
        unit.function("score", &[], true);
    });
    let resolved = context(&project, "a.zry").expect("complete ordered closure");
    let modules = resolved
        .modules()
        .map(|module| (module.path(), module.identity().index()))
        .collect::<Vec<_>>();
    assert_eq!(modules, [("a.zry", 0), ("z.zry", 1)]);
    assert_eq!(
        resolved
            .modules()
            .next()
            .expect("a")
            .imports()
            .next()
            .expect("binding")
            .target()
            .identity()
            .module()
            .index(),
        1
    );
}

#[test]
fn data_and_function_source_indices_are_disjoint() {
    let project = fixtures::project(&["main.zry"], |_, unit| {
        unit.data("Box", &["T"], false, true);
        unit.function("first", &[], true);
        unit.data("Choice", &["T", "E"], true, true);
        unit.function("second", &[], true);
    });
    let resolved = context(&project, "main.zry").expect("interleaved original declarations");
    let module = resolved.modules().next().expect("module");
    let data = module.data_declarations().collect::<Vec<_>>();
    let functions = module.functions().collect::<Vec<_>>();
    assert_eq!(data[0].identity().kind(), DeclarationKind::Struct);
    assert_eq!(data[1].identity().kind(), DeclarationKind::Enum);
    assert_eq!(functions[0].identity().kind(), DeclarationKind::Function);
    assert_eq!(data[0].identity().source_index(), 0);
    assert_eq!(functions[0].identity().source_index(), 0);
    assert_ne!(data[0].identity(), functions[0].identity());
    assert_eq!(data[1].identity().source_index(), 1);
    assert_eq!(functions[1].identity().source_index(), 1);
    assert!(data[0].span().end() < functions[0].span().start());
    assert!(functions[0].span().end() < data[1].span().start());
}
