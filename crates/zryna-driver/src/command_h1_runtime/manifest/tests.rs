mod rejection;

use super::super::{
    CommandH1HostPolicy,
    tests::{SourceFixture, private_input::PrivateInput},
};
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io;

fn pure(name: &str) -> ExecutedCommandH1 {
    let source = SourceFixture::new(name);
    let policy = CommandH1HostPolicy::deny_all();
    source
        .prepare(None, &policy)
        .expect("actual verified pure preparation")
        .execute(&policy)
        .expect("actual consuming engine execution")
}

fn bytes(run: &ExecutedCommandH1) -> Vec<u8> {
    run.manifest_bytes("candidate").expect("retained executed authority manifest")
}

fn value(run: &ExecutedCommandH1) -> Value {
    serde_json::from_slice(&bytes(run)).expect("manifest JSON observation")
}

#[track_caller]
fn rejected(original: &[u8], edit: impl FnOnce(&mut Value)) {
    let mut value: Value = serde_json::from_slice(original).expect("original document");
    edit(&mut value);
    rejected_bytes(original, &serde_json::to_vec(&value).expect("mutated document"));
}

#[track_caller]
fn rejected_bytes(original: &[u8], mutant: &[u8]) {
    let first = decode_command_h1_manifest(mutant).expect_err("closed observation rejects");
    assert_eq!(first, invalid());
    assert_eq!(first, decode_command_h1_manifest(mutant).expect_err("repeat rejection"));
    decode_command_h1_manifest(original).expect("next valid observation recovers");
}

#[test]
fn actual_pure_true_false_manifest_records_exact_identity_wit_limits_and_result() {
    for (name, expected) in
        [("pure-entry", CommandH1RunReturn::Ok), ("pure-false", CommandH1RunReturn::Err)]
    {
        let run = pure(name);
        let bytes = bytes(&run);
        let manifest = decode_command_h1_manifest(&bytes).expect("closed actual manifest");
        assert_eq!(manifest.record(), run.record());
        assert_eq!(manifest.source_path(), "src/main.zry");
        assert_eq!(manifest.component_path(), "wasi-command/candidate.wasm");
        assert_eq!(manifest.input_kind(), "none");
        assert_eq!(manifest.input_byte_count(), 0);
        assert_eq!(
            manifest.record().outcome(),
            &CommandH1Outcome::RunReturned { result: expected }
        );
        assert_eq!(manifest.record().teardown(), CommandH1Teardown::Confirmed);
        let document = &manifest.document;
        assert_eq!(document.source.sha256, hex(run.artifact().source_digest()));
        assert_eq!(document.source.program_binding, hex(run.artifact().program_binding()));
        assert_eq!(document.component.sha256, hex(run.artifact().component_digest()));
        assert_eq!(document.component.packages.len(), 8);
        assert_eq!(document.component.explicit_imports.len(), 13);
        assert_eq!(document.component.resolved_imports.len(), 16);
        assert_eq!(document.component.exports, ["wasi:cli/run@0.2.12"]);
        assert_eq!(document.component.wit_file_count, 34);
        assert!(document.grants.requested.is_empty() && document.grants.effective.is_empty());
        assert_eq!(document.grants.static_quota, [0; 10]);
        assert_eq!(
            document.grants.registry_ceilings,
            [64, 64, 128, 65_536, 16, 256, 64, 128, 65_536, 8_388_608]
        );
        assert_eq!(bytes, self::bytes(&run));
    }
}

#[test]
fn actual_missing_present_empty_and_present_manifest_exclude_value_path_and_value_hash()
-> io::Result<()> {
    let source = SourceFixture::new("environment-match");
    for input in [None, Some(""), Some("private-input-only"), Some("é🙂")] {
        let fixture = PrivateInput::new("MODE", input)?;
        let policy = CommandH1HostPolicy::environment("MODE").expect("current root approval");
        let run = source
            .prepare(Some(&fixture), &policy)
            .expect("actual captured preparation")
            .execute(&policy)
            .expect("actual environment engine execution");
        let encoded = bytes(&run);
        let text = std::str::from_utf8(&encoded).expect("UTF-8 observation");
        assert!(!text.contains(&fixture.path().to_string_lossy().to_string()));
        if let Some(secret) = input.filter(|text| !text.is_empty()) {
            let secret_hash: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
            assert!(!text.contains(secret));
            assert!(!text.contains(&hex(&secret_hash)));
        }
        let manifest = decode_command_h1_manifest(&encoded).expect("closed granted observation");
        assert_eq!(manifest.record(), run.record());
        assert_eq!(manifest.input_kind(), if input.is_some() { "present" } else { "missing" });
        assert_eq!(
            manifest.input_byte_count(),
            u32::try_from(input.map_or(0, str::len)).expect("bound")
        );
        assert_eq!(manifest.document.grants.requested, grant(Some("MODE")));
        assert_eq!(manifest.document.grants.effective, grant(Some("MODE")));
        assert_eq!(manifest.document.grants.static_quota[2..4], [1, 1088]);
        #[cfg(windows)]
        fixture.assert_retained();
        drop(run);
        fixture.assert_released()?;
    }
    Ok(())
}

#[test]
fn actual_revoked_callback_denial_manifest_preserves_admission_metadata_after_run() -> io::Result<()>
{
    let source = SourceFixture::new("environment-match");
    let input = PrivateInput::new("MODE", Some("private-input-only"))?;
    let policy = CommandH1HostPolicy::environment("MODE").expect("initial root approval");
    let run = source
        .prepare(Some(&input), &policy)
        .expect("actual preparation")
        .execute_before_call(&policy, &|| policy.revoke())
        .expect("actual revoked callback run");
    assert_eq!(
        run.record().outcome(),
        &CommandH1Outcome::HostDenial {
            interface: ENVIRONMENT.into(),
            operation: "get-environment".into(),
        }
    );
    assert_eq!(run.record().teardown(), CommandH1Teardown::Confirmed);
    let original = bytes(&run);
    let manifest = decode_command_h1_manifest(&original).expect("actual denial observation");
    assert_eq!(manifest.record(), run.record());
    assert_eq!(manifest.document.execution.run_return, RunReturn::Absent);
    assert!(manifest.document.execution.trap_identity.absent());
    assert!(manifest.document.execution.trap_category.absent());
    assert_eq!(manifest.document.grants.effective, grant(Some("MODE")));
    input.privacy(true)?;
    assert_eq!(bytes(&run), original, "post-run privacy changes do not erase observed denial");
    drop(run);
    input.privacy(false)?;
    input.assert_released()?;
    Ok(())
}

#[test]
fn actual_bounds_fuel_and_host_panic_manifests_preserve_trap_and_teardown_observations() {
    for name in ["language-bounds", "fuel-exhaustion"] {
        let run = pure(name);
        let manifest = decode_command_h1_manifest(&bytes(&run)).expect("actual trapped manifest");
        assert_eq!(manifest.record(), run.record());
        assert_eq!(manifest.document.execution.run_return, RunReturn::Absent);
        assert!(manifest.document.execution.denial.absent());
    }
    let source = SourceFixture::new("pure-entry");
    let policy = CommandH1HostPolicy::deny_all();
    let run = source
        .prepare(None, &policy)
        .expect("real prepared host interruption fixture")
        .execute_before_call(&policy, &|| panic!("fixed command fixture interruption"))
        .expect("actual host panic observation");
    let manifest = decode_command_h1_manifest(&bytes(&run)).expect("unconfirmed panic manifest");
    assert_eq!(
        manifest.record().outcome(),
        &CommandH1Outcome::RuntimeTrap {
            category: CommandH1TrapCategory::HostProcessFailure,
            identity: None,
        }
    );
    assert_eq!(manifest.record().teardown(), CommandH1Teardown::Unconfirmed);
    assert!(!manifest.record().succeeded());
    assert!(manifest.document.execution.denial.absent());
    assert!(manifest.document.execution.trap_identity.absent());
    assert!(pure("pure-entry").record().succeeded());
}

#[test]
fn manifest_path_uses_one_portable_stem_and_decoder_acceptance_grants_no_execution() {
    let run = pure("pure-entry");
    for stem in ["", "../escape", "CON", "a/b", "a.wasm", "a b"] {
        assert_eq!(run.manifest_bytes(stem).expect_err("unsafe stem rejected"), invalid());
    }
    let mut editable = value(&run);
    editable["execution"]["runReturn"] = "err".into();
    let manifest =
        decode_command_h1_manifest(&serde_json::to_vec(&editable).expect("editable JSON"))
            .expect("internally valid observation is not source/run attestation");
    assert_eq!(
        manifest.record().outcome(),
        &CommandH1Outcome::RunReturned { result: CommandH1RunReturn::Err }
    );
    assert!(run.record().succeeded(), "parsed JSON cannot change the consumed run's actual record");
}
