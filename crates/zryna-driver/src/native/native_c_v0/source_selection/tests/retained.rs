use super::*;

#[test]
fn retained_emitter_rejects_transplanted_map_and_unsupported_target() {
    let original = sources();
    let syntax = zryna_syntax::native_c_source_v0::authenticate_sources(&original)
        .expect("original syntax issuer");
    let declarations = zryna_semantics::native_c_v0::verify_report(
        DECLARATIONS,
        &original,
        &syntax,
        &[LibraryMaterial {
            library_id: "fixture-c-v0@0",
            header_bytes: HEADER,
            policy_bytes: POLICY,
        }],
        zryna_syntax::native_c_v0::TARGET,
    )
    .expect("genuine retained declarations");
    let replacement = sources();
    for (map, target, code) in [
        (&replacement, zryna_syntax::native_c_v0::TARGET, "ZRYNA-C4106"),
        (&original, "javascript", "ZRYNA-C4103"),
    ] {
        EMITTER_ENTRIES.with(|entries| entries.set(0));
        let errors = emit_authenticated_handle_entry(map, &declarations, target, "imported")
            .expect_err("retained issuer and selection must reject before emitter");
        assert_eq!(errors[0].code(), code);
        EMITTER_ENTRIES.with(|entries| assert_eq!(entries.get(), 0));
    }
}
