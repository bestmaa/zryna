use super::super::diagnostics::Errors;
use super::super::resources::{Metric, add};
use super::fixtures::{self, Project};
use super::{context, source_text};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Companion {
    Clean,
    Missing,
    Private,
    WrongCase,
    Collision,
    Cycle,
    Unreachable,
    ManyMissing,
}

fn shadows(companion: Companion) -> Project {
    let mut paths = vec!["late.zry", "main.zry", "values.zry"];
    if companion == Companion::Unreachable {
        paths.push("unused.zry");
    }
    fixtures::project(&paths, |path, unit| match path {
        "main.zry" => {
            if companion == Companion::ManyMissing {
                for index in 0..256 {
                    unit.import(
                        &format!("./missing{index:04}.zry"),
                        &[("Unknown", &format!("unresolved{index}"))],
                    );
                }
            }
            unit.import("./values.zry", &[("Box", "T")]);
            unit.import("./late.zry", &[("lateRead", "lateCall")]);
            match companion {
                Companion::Missing => unit.import("./missing.zry", &[("Unknown", "unresolved")]),
                Companion::Private => unit.import("./values.zry", &[("Hidden", "hiddenType")]),
                Companion::WrongCase => unit.import("./values.zry", &[("box", "wrongCaseType")]),
                _ => {}
            }
            if companion == Companion::Collision {
                unit.function("T", &[], true);
            }
            unit.function("owner", &["T"], true);
        }
        "late.zry" => {
            unit.import("./values.zry", &[("Box", "U")]);
            unit.function("lateRead", &["U"], true);
        }
        "values.zry" => {
            if companion == Companion::Cycle {
                unit.import("./main.zry", &[("owner", "mainOwner")]);
            }
            unit.data("Box", &["T"], false, true);
            unit.data("Hidden", &[], false, false);
        }
        _ => unit.function("unused", &[], false),
    })
}

#[test]
fn visible_imported_type_parameter_shadow_rejected() {
    for companion in [
        Companion::Clean,
        Companion::Missing,
        Companion::Private,
        Companion::WrongCase,
        Companion::Collision,
        Companion::Cycle,
        Companion::Unreachable,
        Companion::ManyMissing,
    ] {
        let project = shadows(companion);
        let errors = context(&project, "main.zry").expect_err("project-wide D barrier");
        assert_eq!(errors.len(), 2, "{companion:?}: {errors:?}");
        assert!(
            errors.iter().all(|error| error.code() == "ZRYNA-D7001"),
            "{companion:?}: module candidates must not consume D slots: {errors:?}"
        );
        for (error, token) in errors.iter().zip(["U", "T"]) {
            let at = error.primary_span().expect("original owning parameter");
            let source = project.sources.source(at.file()).expect("source");
            assert_eq!(source_text(&project, at), token);
            let start = source
                .text()
                .find(&format!("<{token} extends"))
                .expect("original owning declaration header")
                + 1;
            assert_eq!(at.start() as usize, start);
            assert_eq!(at.end() as usize, start + 1);
        }
    }
}

#[test]
fn imported_function_name_does_not_become_visible_type() {
    for mode in ["function", "private", "wrong-case", "missing"] {
        let project = fixtures::project(&["main.zry", "values.zry"], |path, unit| {
            if path == "main.zry" {
                let (path, name) = match mode {
                    "function" => ("./values.zry", "identity"),
                    "private" => ("./values.zry", "Hidden"),
                    "wrong-case" => ("./values.zry", "box"),
                    _ => ("./missing.zry", "Box"),
                };
                unit.import(path, &[(name, "T")]);
                unit.import("./values.zry", &[("read", "importedRead")]);
                unit.function("owner", &["T"], true);
            } else {
                unit.function("identity", &["X"], true);
                unit.function("read", &[], true);
                unit.data("Hidden", &[], false, false);
                unit.data("Box", &["X"], false, true);
            }
        });
        let result = context(&project, "main.zry");
        if mode == "function" {
            let resolved = result.expect("function aliases create no visible data type");
            let owner = resolved.modules().next().expect("main").functions().next().expect("owner");
            assert_eq!(resolved.type_parameter(owner.identity(), "T").expect("own T").name(), "T");
        } else {
            let errors = result.expect_err("original target rejected without false D");
            assert_eq!(errors.len(), 1, "{mode}: {errors:?}");
            assert!(errors.iter().all(|error| error.code() == "ZRYNA-M3016"));
        }
    }
}

#[test]
fn parameter_scope_cannot_cross_declarations() {
    let project = fixtures::project(&["main.zry"], |_, unit| {
        unit.function("first", &["T"], true);
        unit.function("second", &["E"], false);
        unit.function("third", &["T"], false);
    });
    let resolved = context(&project, "main.zry").expect("separate original scopes");
    let owners = resolved.modules().next().expect("module").functions().collect::<Vec<_>>();
    let first = resolved.type_parameter(owners[0].identity(), "T").expect("first T");
    let third = resolved.type_parameter(owners[2].identity(), "T").expect("third T");
    assert_eq!(first.identity().declaration(), owners[0].identity());
    assert_eq!(first.identity().index(), 0);
    assert_eq!(source_text(&project, first.bound_span()), "ZrynaValue");
    assert!(resolved.type_parameter(owners[1].identity(), "T").is_none());
    assert!(resolved.type_parameter(owners[0].identity(), "E").is_none());
    assert_ne!(first.identity(), third.identity());
    let other = fixtures::project(&["main.zry"], |_, unit| unit.function("first", &["T"], true));
    let other_context = context(&other, "main.zry").expect("other scope");
    let foreign = other_context
        .modules()
        .next()
        .expect("other module")
        .functions()
        .next()
        .expect("foreign owner")
        .identity();
    assert!(resolved.type_parameter(foreign, "T").is_none());
}

#[test]
fn unused_template_scope_retained() {
    let project = fixtures::project(&["main.zry"], |_, unit| {
        unit.data("UnusedBox", &["T"], false, false);
        unit.function("unusedIdentity", &["U"], false);
        unit.function("score", &[], true);
    });
    let resolved = context(&project, "main.zry").expect("unused original templates retained");
    let module = resolved.modules().next().expect("module");
    let data = module.data_declarations().next().expect("unused data");
    let function = module.functions().next().expect("unused function");
    assert_eq!(data.name(), "UnusedBox");
    assert!(!data.is_exported());
    assert!(!function.is_exported());
    assert_eq!(data.type_parameters().next().expect("T").name(), "T");
    assert_eq!(function.type_parameters().next().expect("U").name(), "U");
    assert_eq!(resolved.syntax().files()[0].functions.len(), 2);
}

#[test]
fn source_order_diagnostics_replayed_deterministically() {
    let project = fixtures::project(&["a.zry", "b.zry", "c.zry"], |path, unit| match path {
        "a.zry" => {
            unit.import("./b.zry", &[("Hidden", "hidden")]);
            unit.import("./missing.zry", &[("Missing", "missing")]);
            unit.function("score", &[], true);
        }
        "b.zry" => {
            unit.import("./a.zry", &[("score", "originalScore")]);
            unit.function("Hidden", &[], false);
        }
        _ => unit.function("unused", &[], false),
    });
    let first = context(&project, "a.zry").expect_err("complete original negatives");
    let second = context(&project, "a.zry").expect_err("deterministic replay");
    assert_eq!(first, second);
    assert_eq!(first.len(), 4);
    let locations = first[..3]
        .iter()
        .map(|error| {
            let span = error.primary_span().expect("original source location");
            (span.file().index(), span.start(), span.end())
        })
        .collect::<Vec<_>>();
    assert!(locations.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(first[3].primary_span().is_none());
    let mut deduplicated = Errors::default();
    let span = first[0].primary_span().expect("source witness");
    for _ in 0..300 {
        deduplicated.at("ZRYNA-M3016", span, "same original witness", "fix the original import");
    }
    assert_eq!(deduplicated.finish().len(), 1);
}

fn missing_imports(count: usize) -> Project {
    fixtures::project(&["main.zry"], |_, unit| {
        for index in 0..count {
            unit.import(
                &format!("./missing{index:04}.zry"),
                &[("Unknown", &format!("unresolved{index}"))],
            );
        }
        unit.function("score", &[], true);
    })
}

#[test]
fn diagnostic_terminal_returns_no_context() {
    let exact = missing_imports(255);
    let exact_errors = context(&exact, "main.zry").expect_err("255 original ordinary errors");
    assert_eq!(exact_errors.len(), 255);
    assert!(exact_errors.iter().all(|error| error.code() == "ZRYNA-M3016"));
    let extra = missing_imports(260);
    let errors = context(&extra, "main.zry").expect_err("no partial context at terminal budget");
    assert_eq!(errors.len(), 256);
    assert!(errors[..255].iter().all(|error| error.code() == "ZRYNA-M3016"));
    assert_eq!(errors[255].code(), "ZRYNA-M7201");
    assert_eq!(
        source_text(&extra, errors[255].primary_span().expect("first extra import")),
        "\"./missing0255.zry\""
    );
    let shadows = fixtures::project(&["main.zry", "values.zry"], |path, unit| {
        if path == "main.zry" {
            for index in 0..260 {
                unit.import(
                    &format!("./missing{index:04}.zry"),
                    &[("Unknown", &format!("unresolved{index}"))],
                );
            }
            unit.import("./values.zry", &[("Box", "T")]);
            for index in 0..260 {
                unit.function(&format!("owner{index:04}"), &["T"], true);
            }
        } else {
            unit.data("Box", &["X"], false, true);
        }
    });
    let errors =
        context(&shadows, "main.zry").expect_err("D slots precede every stored M candidate");
    assert_eq!(errors.len(), 256);
    assert!(errors[..255].iter().all(|error| error.code() == "ZRYNA-D7001"));
    assert_eq!(errors[255].code(), "ZRYNA-M7201");
    let at = errors[255].primary_span().expect("first extra owning parameter");
    assert_eq!(source_text(&shadows, at), "T");
    let source = shadows.sources.source(at.file()).expect("owning source");
    assert_eq!(
        at.start() as usize,
        source.text().find("owner0255<T").expect("canonical first extra declaration")
            + "owner0255<".len()
    );
}

#[test]
fn module_binding_checked_counters_exact_and_extra() {
    // Synthetic counter witnesses test pre-allocation limits independently of source admissibility.
    for (metric, frozen_limit) in [
        (Metric::Modules, 4096),
        (Metric::SourceBytes, 8 * 1024 * 1024),
        (Metric::ImportsPerModule, 4096),
        (Metric::Imports, 65_536),
        (Metric::NamesPerImport, 256),
        (Metric::ImportedNames, 65_536),
        (Metric::DataPerModule, 4096),
        (Metric::Data, 16_384),
        (Metric::FunctionsPerModule, 4096),
        (Metric::Functions, 16_384),
        (Metric::TypesPerModule, 65_536),
        (Metric::Types, 262_144),
    ] {
        assert_eq!(metric.limit(), frozen_limit);
        assert_eq!(
            add(frozen_limit - 1, 1, metric).expect("exact inherited counter"),
            frozen_limit
        );
        let extra = add(frozen_limit, 1, metric)
            .expect_err("first extra cannot allocate a context")
            .diagnostic();
        assert_eq!(extra.code(), "ZRYNA-M7201");
        assert!(extra.message().contains(&(frozen_limit + 1).to_string()));
        assert!(extra.primary_span().is_none());
        let overflow =
            add(usize::MAX, 1, metric).expect_err("checked addition cannot wrap").diagnostic();
        assert_eq!(overflow.code(), "ZRYNA-M7201");
        assert!(overflow.message().contains("checked-add overflow"));
    }
}

#[test]
fn failed_resolution_then_pristine_replay() {
    let project = fixtures::reference();
    let first = context(&project, "main.zry").expect("pristine original context");
    let original = first
        .modules()
        .flat_map(|module| module.data_declarations().chain(module.functions()))
        .map(|declaration| (declaration.identity(), declaration.span()))
        .collect::<Vec<_>>();
    let failed =
        context(&project, "values.zry").expect_err("selected entry has incomplete closure");
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].code(), "ZRYNA-M3016");
    let replay = context(&project, "main.zry").expect("failed context never mutates input");
    let replayed = replay
        .modules()
        .flat_map(|module| module.data_declarations().chain(module.functions()))
        .map(|declaration| (declaration.identity(), declaration.span()))
        .collect::<Vec<_>>();
    assert_eq!(original, replayed);
    assert!(std::ptr::eq(replay.syntax(), &raw const project.syntax));
    assert!(std::ptr::eq(replay.sources(), &raw const project.sources));
}
