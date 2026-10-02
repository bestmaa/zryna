//! Independent valid-module mutations of the unique verified environment callsite.

use wasm_encoder::{Encode as _, Instruction as I};
use wasmparser::{Validator, WasmFeatures};
use zryna_ir::data_ownership_v1::{VerifiedInstructionKind, VerifiedModule};

use super::{Candidate, locate, replacement};

fn indices(candidate: &Candidate) -> (usize, u32) {
    let functions = candidate.program.modules().flat_map(VerifiedModule::functions);
    let caller = functions
        .enumerate()
        .find_map(|(index, function)| {
            function
                .blocks()
                .any(|block| {
                    block.instructions().any(|instruction| {
                        instruction.kind() == VerifiedInstructionKind::EnvironmentLookup
                    })
                })
                .then_some(index)
        })
        .expect("whole verified environment caller");
    let helper = candidate.program.linear32_layouts().types().len() * 2;
    let environment = u32::try_from(helper + 6).expect("bounded helper function index");
    (helper + 1 + caller, environment)
}

fn reject_valid_route(candidate: &Candidate, bytes: &[u8]) {
    Validator::new_with_features(WasmFeatures::WASM1)
        .validate_all(bytes)
        .expect("independent callsite mutation remains valid WebAssembly 1.0");
    let diagnostic = super::super::super::language_audit::audit(bytes, &candidate.program)
        .expect_err("only the sole verified environment callsite may use the helper");
    assert_eq!(diagnostic.code(), "ZRYNA-W4103");
    super::super::super::language_audit::audit(&candidate.core, &candidate.program)
        .expect("unmodified full command audit recovers");
}

#[test]
fn missing_verified_environment_call_rejects_valid_wasm() {
    let candidate = Candidate::new("environment-match");
    let (caller, environment) = indices(&candidate);
    let call = format!("call {environment}");
    let bytes = candidate.mutate_body(caller, |body, operations| {
        let index = locate(operations, &[&call], 0);
        replacement(body, &operations[index], &I::I32Const(0));
    });
    reject_valid_route(&candidate, &bytes);
}

#[test]
fn repeated_environment_call_at_verified_caller_rejects_valid_wasm() {
    let candidate = Candidate::new("environment-helper");
    let (caller, environment) = indices(&candidate);
    let call = format!("call {environment}");
    let bytes = candidate.mutate_body(caller, |body, operations| {
        let index = locate(operations, &[&call], 0);
        let mut repeated = Vec::new();
        I::Call(environment).encode(&mut repeated);
        I::Drop.encode(&mut repeated);
        I::Call(environment).encode(&mut repeated);
        body.splice(operations[index].start..operations[index].end, repeated);
    });
    reject_valid_route(&candidate, &bytes);
}

#[test]
fn rogue_environment_call_in_clone_helper_rejects_valid_wasm() {
    let candidate = Candidate::new("environment-match");
    let (_, environment) = indices(&candidate);
    let bytes = candidate.mutate_body(0, |body, operations| {
        let mut added = Vec::new();
        I::Call(environment).encode(&mut added);
        I::Drop.encode(&mut added);
        body.splice(operations[0].start..operations[0].start, added);
    });
    reject_valid_route(&candidate, &bytes);
}
