use super::super::DeclarationKind;
use super::fixtures;
use super::{assert_error, context, source_text};

#[test]
fn exact_function_template_import_preserves_identity() {
    let project = fixtures::project(&["main.zry", "values.zry"], |path, unit| {
        if path == "main.zry" {
            unit.import("./values.zry", &[("identity", "importedIdentity")]);
            unit.function("score", &[], true);
        } else {
            unit.function("identity", &["T"], true);
        }
    });
    let resolved = context(&project, "main.zry").expect("original generic function import");
    let binding = resolved.modules().next().expect("main").imports().next().expect("binding");
    let original = resolved.modules().nth(1).expect("values").functions().next().expect("function");
    assert_eq!(binding.local_name(), "importedIdentity");
    assert_eq!(binding.imported_name(), "identity");
    assert_eq!(binding.target().identity(), original.identity());
    assert_eq!(binding.target().type_parameters().next().expect("T").name(), "T");
    assert!(binding.target().is_exported());
    assert_eq!(source_text(&project, binding.local_span()), "importedIdentity");
}

#[test]
fn exact_data_template_import_preserves_identity() {
    let project = fixtures::project(&["main.zry", "values.zry"], |path, unit| {
        if path == "main.zry" {
            unit.import("./values.zry", &[("Box", "LocalBox"), ("Choice", "LocalChoice")]);
            unit.function("score", &[], true);
        } else {
            unit.data("Box", &["T"], false, true);
            unit.data("Choice", &["T", "E"], true, true);
        }
    });
    let resolved = context(&project, "main.zry").expect("both original nominal kinds");
    let main = resolved.modules().next().expect("main");
    let originals =
        resolved.modules().nth(1).expect("values").data_declarations().collect::<Vec<_>>();
    let bindings = main.imports().collect::<Vec<_>>();
    assert_eq!(bindings[0].target().identity(), originals[0].identity());
    assert_eq!(bindings[1].target().identity(), originals[1].identity());
    assert_eq!(bindings[0].target().identity().kind(), DeclarationKind::Struct);
    assert_eq!(bindings[1].target().identity().kind(), DeclarationKind::Enum);
    assert!(main.data_declarations().next().is_none());
    assert_eq!(bindings[1].target().type_parameters().count(), 2);
}

#[test]
fn diamond_imports_keep_one_original_declaration() {
    let project =
        fixtures::project(&["main.zry", "right.zry", "values.zry", "left.zry"], |path, unit| {
            match path {
                "main.zry" => {
                    unit.import("./left.zry", &[("readLeft", "left")]);
                    unit.import("./right.zry", &[("readRight", "right")]);
                    unit.function("score", &[], true);
                }
                "left.zry" | "right.zry" => {
                    unit.import(
                        "./values.zry",
                        &[("Box", "LocalBox"), ("identity", "localIdentity")],
                    );
                    unit.function(
                        if path == "left.zry" { "readLeft" } else { "readRight" },
                        &[],
                        true,
                    );
                }
                _ => {
                    unit.data("Box", &["T"], false, true);
                    unit.function("identity", &["T"], true);
                }
            }
        });
    let resolved = context(&project, "main.zry").expect("complete diamond");
    let left = resolved.modules().find(|module| module.path() == "left.zry").expect("left");
    let right = resolved.modules().find(|module| module.path() == "right.zry").expect("right");
    let left_targets =
        left.imports().map(|binding| binding.target().identity()).collect::<Vec<_>>();
    let right_targets =
        right.imports().map(|binding| binding.target().identity()).collect::<Vec<_>>();
    assert_eq!(left_targets, right_targets);
    let original = resolved.modules().find(|module| module.path() == "values.zry").expect("values");
    assert_eq!(original.data_declarations().count(), 1);
    assert_eq!(original.functions().count(), 1);
    assert_eq!(left.data_declarations().count(), 0);
    assert_eq!(right.data_declarations().count(), 0);
}

#[test]
fn ordinary_nongeneric_import_identity_preserved() {
    let project = fixtures::project(&["main.zry", "values.zry"], |path, unit| {
        if path == "main.zry" {
            unit.import("./values.zry", &[("read", "importedRead")]);
            unit.function("score", &[], true);
        } else {
            unit.function("read", &[], true);
        }
    });
    let resolved = context(&project, "main.zry").expect("ordinary import binding");
    let target =
        resolved.modules().next().expect("main").imports().next().expect("binding").target();
    assert_eq!(target.name(), "read");
    assert_eq!(target.identity().module().index(), 1);
    assert_eq!(target.identity().source_index(), 0);
    assert_eq!(target.type_parameters().count(), 0);
}

#[test]
fn absent_authenticated_import_target_rejected() {
    let project = fixtures::project(&["main.zry"], |_, unit| {
        unit.import("./missing.zry", &[("read", "importedRead")]);
        unit.function("score", &[], true);
    });
    assert_error(&project, "main.zry", "ZRYNA-M3016", "\"./missing.zry\"");
}

#[test]
fn wrong_case_import_name_rejected() {
    let project = fixtures::project(&["main.zry", "values.zry"], |path, unit| {
        if path == "main.zry" {
            unit.import("./values.zry", &[("box", "LocalBox")]);
            unit.function("score", &[], true);
        } else {
            unit.data("Box", &["T"], false, true);
        }
    });
    assert_error(&project, "main.zry", "ZRYNA-M3016", "box");
}

#[test]
fn private_target_not_importable() {
    let project = fixtures::project(&["main.zry", "values.zry"], |path, unit| {
        if path == "main.zry" {
            unit.import("./values.zry", &[("Hidden", "importedHidden")]);
            unit.function("score", &[], true);
        } else {
            unit.function("Hidden", &["T"], false);
        }
    });
    assert_error(&project, "main.zry", "ZRYNA-M3016", "Hidden");
}

#[test]
fn import_alias_is_not_implicitly_reexported() {
    let project =
        fixtures::project(&["main.zry", "middle.zry", "values.zry"], |path, unit| match path {
            "main.zry" => {
                unit.import("./middle.zry", &[("identity", "importedIdentity")]);
                unit.function("score", &[], true);
            }
            "middle.zry" => {
                unit.import("./values.zry", &[("identity", "identity")]);
                unit.function("bridge", &[], true);
            }
            _ => unit.function("identity", &["T"], true),
        });
    assert_error(&project, "main.zry", "ZRYNA-M3016", "identity");
}

#[test]
fn module_cycle_rejected_without_recursion() {
    let project = fixtures::project(&["main.zry", "values.zry"], |path, unit| {
        if path == "main.zry" {
            unit.import("./values.zry", &[("other", "other")]);
            unit.function("score", &[], true);
        } else {
            unit.import("./main.zry", &[("score", "rootScore")]);
            unit.function("other", &[], true);
        }
    });
    assert_error(&project, "main.zry", "ZRYNA-M3016", "\"./main.zry\"");
}

#[test]
fn unreachable_source_module_rejected() {
    let project =
        fixtures::project(&["main.zry", "unused.zry"], |_, unit| unit.function("score", &[], true));
    let errors = context(&project, "main.zry").expect_err("no incomplete closure");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "ZRYNA-M3016");
    assert!(errors[0].message().contains("unused.zry"));
    assert!(errors[0].primary_span().is_none());
}

#[test]
fn folded_callable_alias_collision_rejected() {
    let local = fixtures::project(&["main.zry"], |_, unit| {
        unit.function("Read", &[], true);
        unit.function("read", &[], true);
    });
    assert_error(&local, "main.zry", "ZRYNA-M3002", "read");
    let project = fixtures::project(&["main.zry", "values.zry"], |path, unit| {
        if path == "main.zry" {
            unit.import("./values.zry", &[("other", "Score")]);
            unit.function("score", &[], true);
        } else {
            unit.function("other", &[], true);
        }
    });
    assert_error(&project, "main.zry", "ZRYNA-M3002", "Score");
}
