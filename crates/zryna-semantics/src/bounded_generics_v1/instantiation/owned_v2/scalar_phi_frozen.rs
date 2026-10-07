//! Frozen scalar branch observations, source bytes and independent hostile wire claims.
use super::tests::with_claim;
use zryna_ir::generic_v1::{Failure, owned_v2, raw};
const VALUES: &str = include_str!("../../../../../../tests/m7-generic-owned-scalar-phi/values.zry");
const MAIN: &str = include_str!("../../../../../../tests/m7-generic-owned-scalar-phi/main.zry");

fn write_requested_dto(files: &[(&str, &str)]) {
    if let Some(dir) = std::env::var_os("ZRYNA_GENERIC_SCALAR_PHI_FIXTURE_DIR") {
        std::fs::write(
            std::path::PathBuf::from(dir).join(format!("{}-reference.json", files.len())),
            serde_json::to_vec_pretty(&crate::bounded_generics_v1::tests::body_fixtures::snapshot(
                files,
            ))
            .expect("DTO"),
        )
        .expect("explicit owned fixture destination");
    }
}

#[test]
fn frozen_scalar_phi_program_seals_single_and_imported_six_paths() {
    for cross in [false, true] {
        let joined = format!("{VALUES}\n{MAIN}");
        let imported = format!(
            "import {{discard,singleSelect,doubleSelect,earlySelect,nestedSelect,loopSelect,optionSelect}} from \"./values.zry\";\n{MAIN}"
        );
        let files = if cross {
            vec![("main.zry", imported.as_str()), ("values.zry", VALUES)]
        } else {
            vec![("main.zry", joined.as_str())]
        };
        write_requested_dto(&files);
        with_claim(&files, |claim, syntax, sources, linear, linux| {
            let abi = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux).expect("ABI"),
                linear,
                linux,
            )
            .expect("runtime");
            let check = |claim: &owned_v2::raw::Program| {
                owned_v2::verify(
                    owned_v2::wire::decode(&owned_v2::wire::encode(claim).expect("wire"))
                        .expect("decode"),
                    syntax,
                    sources,
                    sources.verify_file_id(0).expect("entry"),
                    linear,
                    linux,
                    &abi,
                )
                .map(|_| ())
            };
            check(&claim).expect("genuine six-path source seal");
            let (fi, bi) = claim
                .graph
                .functions
                .iter()
                .enumerate()
                .find_map(|(fi, f)| {
                    f.blocks
                        .iter()
                        .enumerate()
                        .skip(1)
                        .find(|(_, b)| {
                            b.parameters.len() == 2
                                && b.parameters.iter().any(|p| p.ty == raw::Type::Stored(0))
                        })
                        .map(|(bi, _)| (fi, bi))
                })
                .expect("bool/i32 ordinary join");
            for mutation in 0..7 {
                let mut hostile = claim.clone();
                if mutation < 4 {
                    let f = &mut hostile.graph.functions[fi];
                    let target = f.blocks[bi].id;
                    let edge = f
                        .blocks
                        .iter_mut()
                        .find_map(|b| {
                            if let raw::Terminator::Jump(edge) = &mut b.terminator {
                                (edge.target == target).then_some(edge)
                            } else {
                                None
                            }
                        })
                        .expect("join edge");
                    match mutation {
                        0 => {
                            edge.arguments.pop();
                        }
                        1 => edge.arguments.swap(0, 1),
                        2 => edge.arguments[0] = u32::MAX,
                        _ => edge.target = 0,
                    }
                } else {
                    let step = hostile
                        .plans
                        .iter_mut()
                        .flat_map(|p| &mut p.steps)
                        .find(|s| s.failure && !s.cleanup.is_empty())
                        .expect("fallible retained owner");
                    match mutation {
                        4 => {
                            step.cleanup.pop();
                        }
                        5 => step.cleanup.push(step.cleanup[0]),
                        _ => step.cleanup.push(u32::MAX),
                    }
                }
                assert!(matches!(check(&hostile), Err(Failure::Diagnostics(_))));
                check(&claim).expect("genuine recovery");
            }
        });
    }
}

#[test]
fn scalar_phi_keywords_newlines_unicode_and_statement_bytes_are_authenticated() {
    use crate::bounded_generics_v1::tests::body_fixtures::snapshot;
    use zryna_source::{SourceFileInput, SourceMap};
    for imported in [false, true] {
        let main = if imported {
            format!(
                "import {{discard,singleSelect,doubleSelect,earlySelect,nestedSelect,loopSelect,optionSelect}} from \"./values.zry\";\n{MAIN}"
            )
        } else {
            format!("{VALUES}\n{MAIN}")
        };
        let files = if imported {
            vec![("main.zry", main.as_str()), ("values.zry", VALUES)]
        } else {
            vec![("main.zry", main.as_str())]
        };
        let dto = snapshot(&files);
        let original = files
            .iter()
            .map(|(path, text)| SourceFileInput { path: (*path).into(), text: (*text).into() })
            .collect::<Vec<_>>();
        let sources = SourceMap::build(original.clone()).expect("source");
        zryna_syntax::v5::verify_snapshot(dto.clone(), &sources).expect("original UTF8/newline");
        let affected = usize::from(imported);
        for (from, to) in [
            ("if\n", "іf\n"),
            ("if\n", "i\nf"),
            ("if\n", "let\n"),
            ("一", "二"),
            ("code=11", "code=12"),
            ("else", "elsе"),
        ] {
            let mut hostile = original.clone();
            hostile[affected].text = hostile[affected].text.replace(from, to);
            assert_ne!(hostile[affected].text, original[affected].text, "mutation present");
            assert!(
                zryna_syntax::v5::verify_snapshot(
                    dto.clone(),
                    &SourceMap::build(hostile).expect("valid hostile UTF8")
                )
                .is_err()
            );
            zryna_syntax::v5::verify_snapshot(dto.clone(), &sources).expect("original recovery");
        }
    }
}
