use crate::corpus;
use serde_json::{Value, json};
use zryna_diagnostics::{Diagnostic, render_json};
use zryna_source::SourceMap;

pub fn diagnostics(left: &[Diagnostic], right: &[Diagnostic], sources: &SourceMap) -> Value {
    assert!(!left.is_empty(), "rejection must contain actual diagnostics");
    let left =
        render_json(left, sources).expect("worker diagnostics bound to the original sources");
    let right =
        render_json(right, sources).expect("native diagnostics bound to the original sources");
    assert_eq!(
        left, right,
        "exact diagnostic codes, severity, messages, guidance, order and byte spans"
    );
    serde_json::from_str(&left).expect("rendered diagnostic JSON")
}

pub fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics.iter().map(Diagnostic::code).collect()
}

pub fn m1(left: &zryna_ir::VerifiedProgram, right: &zryna_ir::VerifiedProgram) {
    let left = left.functions().collect::<Vec<_>>();
    let right = right.functions().collect::<Vec<_>>();
    assert_eq!(left.len(), right.len());
    for (left, right) in left.iter().zip(&right) {
        assert_eq!(left.export_name(), right.export_name());
        assert_eq!(left.parameters(), right.parameters());
        assert_eq!(left.return_type(), right.return_type());
        assert_eq!(left.expressions(), right.expressions());
        assert_eq!(left.body(), right.body());
    }
}

pub fn m2(
    left: &zryna_ir::control_flow_v1::VerifiedProgram,
    right: &zryna_ir::control_flow_v1::VerifiedProgram,
) {
    assert_eq!(left.entry_module(), right.entry_module());
    assert_eq!(left.scalar_abi(), right.scalar_abi());
    let left_modules = left.modules().collect::<Vec<_>>();
    let right_modules = right.modules().collect::<Vec<_>>();
    assert_eq!(left_modules.len(), right_modules.len());
    for (left, right) in left_modules.iter().zip(&right_modules) {
        assert_eq!(left.id(), right.id());
        assert_eq!(left.source_file(), right.source_file());
        let left = left.functions().collect::<Vec<_>>();
        let right = right.functions().collect::<Vec<_>>();
        assert_eq!(left.len(), right.len());
        for (left, right) in left.iter().zip(&right) {
            assert_eq!(left.id(), right.id());
            assert_eq!(
                left.parameters().collect::<Vec<_>>(),
                right.parameters().collect::<Vec<_>>()
            );
            assert_eq!(left.result(), right.result());
            assert_eq!(left.span(), right.span());
            let left = left.blocks().collect::<Vec<_>>();
            let right = right.blocks().collect::<Vec<_>>();
            assert_eq!(left.len(), right.len());
            for (left, right) in left.iter().zip(&right) {
                assert_eq!(left.id(), right.id());
                assert_eq!(
                    left.parameters().collect::<Vec<_>>(),
                    right.parameters().collect::<Vec<_>>()
                );
                let a = left.instructions().collect::<Vec<_>>();
                let b = right.instructions().collect::<Vec<_>>();
                assert_eq!(a.len(), b.len());
                for (a, b) in a.iter().zip(&b) {
                    assert_eq!(a.result(), b.result());
                    assert_eq!(a.ty(), b.ty());
                    assert_eq!(a.span(), b.span());
                    assert_eq!(a.kind(), b.kind());
                }
                assert_eq!(left.terminator().span(), right.terminator().span());
                assert_eq!(left.terminator().kind(), right.terminator().kind());
            }
        }
    }
}

pub fn m3(
    left: &zryna_semantics::data_ownership_v1::VerifiedProgram,
    right: &zryna_semantics::data_ownership_v1::VerifiedProgram,
) {
    let left = left.verified_ir();
    let right = right.verified_ir();
    assert_eq!(left.scalar_abi(), right.scalar_abi());
    assert_eq!(left.runtime_contract(), right.runtime_contract());
    for (a, b) in [
        (left.linear32_layouts(), right.linear32_layouts()),
        (left.linux_x86_64_layouts(), right.linux_x86_64_layouts()),
    ] {
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert_eq!(
            format!("{:?}", a.types().collect::<Vec<_>>()),
            format!("{:?}", b.types().collect::<Vec<_>>())
        );
    }
    let a = left.modules().collect::<Vec<_>>();
    let b = right.modules().collect::<Vec<_>>();
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(&b) {
        assert_eq!(a.id(), b.id());
        let a = a.functions().collect::<Vec<_>>();
        let b = b.functions().collect::<Vec<_>>();
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(&b) {
            assert_eq!(a.id(), b.id());
            assert_eq!(a.result_type(), b.result_type());
            assert_eq!(a.parameters().collect::<Vec<_>>(), b.parameters().collect::<Vec<_>>());
            assert_eq!(
                a.borrow_parameters().collect::<Vec<_>>(),
                b.borrow_parameters().collect::<Vec<_>>()
            );
            // Public immutable Debug views include places, cleanup roles and retained verifier
            // indices. They are test observations only, never a production serializer/protocol.
            assert_eq!(
                format!("{:?}", a.places().collect::<Vec<_>>()),
                format!("{:?}", b.places().collect::<Vec<_>>())
            );
            assert_eq!(
                format!("{:?}", a.cleanup_plans().collect::<Vec<_>>()),
                format!("{:?}", b.cleanup_plans().collect::<Vec<_>>())
            );
            let a = a.blocks().collect::<Vec<_>>();
            let b = b.blocks().collect::<Vec<_>>();
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(&b) {
                assert_eq!(a.id(), b.id());
                assert_eq!(a.parameters().collect::<Vec<_>>(), b.parameters().collect::<Vec<_>>());
                let ai = a.instructions().collect::<Vec<_>>();
                let bi = b.instructions().collect::<Vec<_>>();
                assert_eq!(ai.len(), bi.len());
                for (a, b) in ai.iter().zip(&bi) {
                    assert_eq!(a.kind(), b.kind());
                    assert_eq!(a.result(), b.result());
                    assert_eq!(a.result_type(), b.result_type());
                    assert_eq!(a.backend_instruction(), b.backend_instruction());
                    assert_eq!(
                        format!("{a:?}"),
                        format!("{b:?}"),
                        "complete immutable instruction view"
                    );
                }
                assert_eq!(
                    a.terminator().backend_terminator(),
                    b.terminator().backend_terminator()
                );
                assert_eq!(format!("{:?}", a.terminator()), format!("{:?}", b.terminator()));
            }
        }
    }
}

pub struct Artifacts {
    pub javascript: String,
    pub wasm: Vec<u8>,
    pub object: Vec<u8>,
}

impl Artifacts {
    pub fn compare(&self, right: &Self) -> Value {
        assert_eq!(self.javascript, right.javascript, "JavaScript bytes");
        assert_eq!(self.wasm, right.wasm, "WebAssembly bytes");
        assert_eq!(self.object, right.object, "ELF object bytes");
        json!({"javascript":{"sha256":corpus::digest(self.javascript.as_bytes()),"bytes":self.javascript.len()},
            "webassembly":{"sha256":corpus::digest(&self.wasm),"bytes":self.wasm.len()},
            "native-object":{"sha256":corpus::digest(&self.object),"bytes":self.object.len()}})
    }
}

pub fn emit1(program: &zryna_ir::VerifiedProgram) -> Artifacts {
    let mir = zryna_native_mir::lower(program).expect("sealed M1 MIR");
    Artifacts {
        javascript: zryna_backend_javascript::emit(program).expect("M1 JS").source,
        wasm: zryna_backend_webassembly::emit(program).expect("M1 Wasm").bytes().to_vec(),
        object: zryna_backend_native::emit_object(&mir, target()).expect("M1 ELF").bytes().to_vec(),
    }
}
pub fn emit2(program: &zryna_ir::control_flow_v1::VerifiedProgram) -> Artifacts {
    let mir = zryna_native_mir::control_flow_v1::lower(program).expect("sealed M2 MIR");
    Artifacts {
        javascript: zryna_backend_javascript::emit_control_flow(program).expect("M2 JS").source,
        wasm: zryna_backend_webassembly::emit_control_flow(program)
            .expect("M2 Wasm")
            .bytes()
            .to_vec(),
        object: zryna_backend_native::control_flow_v1::emit_object(&mir, target())
            .expect("M2 ELF")
            .bytes()
            .to_vec(),
    }
}
pub fn emit3(program: &zryna_semantics::data_ownership_v1::VerifiedProgram) -> Artifacts {
    let ir = program.verified_ir();
    let mir = zryna_native_mir::data_ownership_v1::lower(ir, program.runtime_abi())
        .expect("sealed M3 MIR");
    Artifacts {
        javascript: zryna_backend_javascript::emit_data_ownership(ir, program.runtime_abi())
            .expect("M3 JS")
            .source,
        wasm: zryna_backend_webassembly::emit_data_ownership(ir, program.runtime_abi())
            .expect("M3 Wasm")
            .bytes()
            .to_vec(),
        object: zryna_backend_native::data_ownership_v1::emit_object(&mir, target())
            .expect("M3 ELF")
            .bytes()
            .to_vec(),
    }
}
fn target() -> zryna_backend_native::LinuxX8664ObjectTarget {
    zryna_backend_native::select_object_target(zryna_backend_native::NATIVE_OBJECT_TARGET)
        .expect("existing fixed ELF target")
}
