use super::*;

#[test]
fn bool32_c_client_invalid_low_width_carrier_terminates_without_scalar_result() {
    let fixture = Fixture::new();
    let root = fixture.output();
    let toolchain = toolchain();
    let artifact = emit(&capture::boolean_export());
    for value in [false, true] {
        let invocation = prepare_scalar_export(
            &artifact,
            "add",
            &[ScalarValue::Bool(value)],
            &root,
            &toolchain,
            NativeProcessLimits::default(),
        )
        .expect("canonical Boolean invocation");
        assert_eq!(
            invocation.run(&root, NativeProcessLimits::default()).expect("Boolean result"),
            ScalarOutcome::Returned { value: ScalarValue::Bool(value) }
        );
    }
    for invalid in [2_u32, u32::MAX] {
        let client = format!(
            "{}\n#include <stdio.h>\nint main(void) {{ printf(\"%u\", zryna_c_v0_e_add(UINT32_C({invalid}))); return 0; }}\n",
            artifact.header()
        );
        let bytes = linked_client(&artifact, &client, &root, &toolchain)
            .expect("hostile raw client is independently compiled");
        let output = observe(&bytes, &root);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        fixture.assert_empty();
    }
}
