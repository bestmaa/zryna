use wasm_encoder::{
    Alias, CanonicalFunctionSection, CanonicalOption, ComponentAliasSection, ComponentExportKind,
    ExportKind, InstanceSection, ModuleArg,
    reencode::{ReencodeComponent as _, RoundtripReencoder},
};
use wasmparser::{ComponentAlias, ComponentExternalKind, ExternalKind, Instance, Parser, Payload};

use super::{Candidate, replace};

#[test]
fn alternate_canonical_encoding_and_option_order_reject_valid_components() {
    let candidate = Candidate::new("environment-match");
    let at = Parser::new(0)
        .parse_all(candidate.artifact.bytes())
        .find_map(|payload| {
            if let Payload::ComponentCanonicalSection(functions) =
                payload.expect("canonical syntax")
                && functions.clone().into_iter().any(|function| {
                    matches!(
                        function.expect("canonical"),
                        wasmparser::CanonicalFunction::Lower { .. }
                    )
                })
            {
                return Some(functions.range());
            }
            None
        })
        .expect("environment canonical lowering");
    for options in [
        vec![CanonicalOption::UTF16, CanonicalOption::Memory(0), CanonicalOption::Realloc(0)],
        vec![CanonicalOption::Memory(0), CanonicalOption::Realloc(0), CanonicalOption::UTF8],
        vec![CanonicalOption::Memory(0), CanonicalOption::Realloc(0)],
    ] {
        let mut functions = CanonicalFunctionSection::new();
        functions.lower(0, options);
        candidate.reject_valid(
            &replace(candidate.artifact.bytes(), at.clone(), &functions),
            "ZRYNA-W4104",
        );
    }
}

#[test]
fn alias_to_other_environment_operation_rejects_valid_component() {
    let candidate = Candidate::new("environment-match");
    let mut selected = None;
    for payload in Parser::new(0).parse_all(candidate.artifact.bytes()) {
        if let Payload::ComponentAliasSection(aliases) = payload.expect("alias syntax") {
            let original =
                aliases.clone().into_iter().collect::<Result<Vec<_>, _>>().expect("aliases");
            if original.iter().any(|alias| {
                matches!(
                    alias,
                    ComponentAlias::InstanceExport {
                        kind: ComponentExternalKind::Func,
                        name: "get-environment",
                        ..
                    }
                )
            }) {
                let mut changed = ComponentAliasSection::new();
                let mut encoder = RoundtripReencoder;
                for alias in original {
                    let alias = match alias {
                        ComponentAlias::InstanceExport {
                            kind: ComponentExternalKind::Func,
                            instance_index,
                            name: "get-environment",
                        } => Alias::InstanceExport {
                            instance: instance_index,
                            kind: ComponentExportKind::Func,
                            name: "get-arguments",
                        },
                        other => encoder.component_alias(other).expect("unchanged alias"),
                    };
                    changed.alias(alias);
                }
                selected = Some((aliases.range(), changed));
                break;
            }
        }
    }
    let (at, aliases) = selected.expect("actual environment alias");
    candidate.reject_valid(&replace(candidate.artifact.bytes(), at, &aliases), "ZRYNA-W4104");
}

#[test]
fn extra_host_export_and_reordered_named_instances_reject_valid_components() {
    let candidate = Candidate::new("environment-match");
    for extra_export in [true, false] {
        let mut selected = None;
        for payload in Parser::new(0).parse_all(candidate.artifact.bytes()) {
            if let Payload::InstanceSection(instances) = payload.expect("instance syntax") {
                let original = instances
                    .clone()
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                    .expect("instances");
                let matches_target = original.iter().any(|instance| {
                    if extra_export {
                        matches!(instance, Instance::FromExports(_))
                    } else {
                        matches!(instance, Instance::Instantiate { module_index: 1, .. })
                    }
                });
                if !matches_target {
                    continue;
                }
                let mut changed = InstanceSection::new();
                for instance in original {
                    match instance {
                        Instance::FromExports(exports) => {
                            let [export] = exports.as_ref() else {
                                panic!("one exact host export");
                            };
                            assert_eq!(export.kind, ExternalKind::Func);
                            let mut entries = vec![(export.name, ExportKind::Func, export.index)];
                            if extra_export {
                                entries.push(("unused", ExportKind::Func, export.index));
                            }
                            changed.export_items(entries);
                        }
                        Instance::Instantiate { module_index, args } => {
                            let mut args = args
                                .iter()
                                .map(|arg| (arg.name, ModuleArg::Instance(arg.index)))
                                .collect::<Vec<_>>();
                            if !extra_export && module_index == 1 {
                                args.reverse();
                            }
                            changed.instantiate(module_index, args);
                        }
                    }
                }
                selected = Some((instances.range(), changed));
                break;
            }
        }
        let (at, instances) = selected.expect("actual host/language instances");
        candidate.reject_valid(&replace(candidate.artifact.bytes(), at, &instances), "ZRYNA-W4104");
    }
}
