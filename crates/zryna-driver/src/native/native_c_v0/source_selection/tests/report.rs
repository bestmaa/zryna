use super::*;

fn report_candidate(extra: bool) -> Vec<u8> {
    let mut document: serde_json::Value = serde_json::from_slice(DECLARATIONS).expect("fixture");
    document["sources"] = (0..256).map(|index| {
        serde_json::json!({"path":format!("budget/{index:03}.zry"), "sha256":"A".repeat(64)})
    }).collect::<Vec<_>>().into();
    if extra {
        document["libraries"][0]["version"] = "".into();
    }
    document.sort_all_objects();
    let mut wire = serde_json::to_vec(&document).expect("canonical test wire");
    wire.push(b'\n');
    wire
}

#[test]
fn exact_256_declaration_candidates_retain_all_errors_before_real_emitter() {
    report(false);
}

#[test]
fn first_extra_257_declaration_candidate_replaces_slot_256_before_real_emitter() {
    report(true);
}

fn report(extra: bool) {
    let sources = sources();
    let syntax = zryna_syntax::native_c_source_v0::authenticate_sources(&sources)
        .expect("original source issuer");
    let materials = [LibraryMaterial {
        library_id: "fixture-c-v0@0",
        header_bytes: HEADER,
        policy_bytes: POLICY,
    }];
    EMITTER_ENTRIES.with(|entries| entries.set(0));
    let errors = emit_handle_entry(
        &sources,
        &report_candidate(extra),
        &materials,
        zryna_syntax::native_c_v0::TARGET,
        "imported",
    )
    .expect_err("atomic declaration rejection");
    assert_eq!(errors.len(), 256);
    assert!(errors[..255].iter().all(|error| error.code() == "ZRYNA-C4101"));
    assert_eq!(errors[255].code(), if extra { "ZRYNA-C4108" } else { "ZRYNA-C4101" });
    assert_eq!(
        errors.iter().filter(|error| error.code() == "ZRYNA-C4108").count(),
        usize::from(extra)
    );
    EMITTER_ENTRIES.with(|entries| assert_eq!(entries.get(), 0));
    let valid = zryna_semantics::native_c_v0::verify_report(
        DECLARATIONS,
        &sources,
        &syntax,
        &materials,
        zryna_syntax::native_c_v0::TARGET,
    )
    .expect("later valid input retains no diagnostics");
    assert!(valid.belongs_to(&sources));
    assert_eq!(valid.declaration_bytes(), DECLARATIONS);
    eprintln!(
        "417-declaration-report-evidence {}",
        serde_json::json!({"candidate_count":256+usize::from(extra),
        "report_count":errors.len(),"terminal_code":errors[255].code(),"pre_emitter_entries":0,
        "original_source_issuer_retained":true,"later_valid_declaration_verified":true})
    );
}
