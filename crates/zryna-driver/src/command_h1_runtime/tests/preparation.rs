use super::*;
use std::io;

#[test]
fn omitted_wrong_key_and_pure_extra_input_reject_before_consuming_execution() -> io::Result<()> {
    let source = SourceFixture::new("environment-match");
    let approved = CommandH1HostPolicy::environment("MODE").expect("root approval");
    rejected_preparation(&source, None, &approved, "ZRYNA-C4102");
    let input = PrivateInput::new("MORE", Some("on"))?;
    rejected_preparation(&source, Some(&input), &approved, "ZRYNA-D4101");
    let input = PrivateInput::new("MODE", Some("on"))?;
    rejected_preparation(&source, Some(&input), &CommandH1HostPolicy::deny_all(), "ZRYNA-C4102");
    let wrong = CommandH1HostPolicy::environment("MORE").expect("distinct approval");
    rejected_preparation(&source, Some(&input), &wrong, "ZRYNA-C4102");
    let pure = SourceFixture::new("pure-entry");
    rejected_preparation(&pure, Some(&input), &CommandH1HostPolicy::deny_all(), "ZRYNA-C4102");
    rejected_preparation(&pure, None, &approved, "ZRYNA-C4102");
    fresh_recovery();
    Ok(())
}

#[test]
fn first_extra_value_byte_and_nonprivate_input_reject_before_engine() -> io::Result<()> {
    let source = SourceFixture::new("environment-match");
    let policy = CommandH1HostPolicy::environment("MODE").expect("root approval");
    for value in ["a".repeat(1025), format!("{}x", "🙂".repeat(256))] {
        assert_eq!(value.len(), 1025);
        let input = PrivateInput::new("MODE", Some(&value))?;
        rejected_preparation(&source, Some(&input), &policy, "ZRYNA-D4101");
    }
    let input = PrivateInput::new("MODE", Some("on"))?;
    input.privacy(true)?;
    rejected_preparation(&source, Some(&input), &policy, "ZRYNA-D4101");
    input.privacy(false)?;
    let run = source
        .prepare(Some(&input), &policy)
        .expect("fresh private input recovery")
        .execute(&policy)
        .expect("actual recovery execution");
    returned(&run, CommandH1RunReturn::Ok);
    drop(run);
    input.assert_released()?;
    Ok(())
}

#[test]
fn revoked_or_equal_key_distinct_policy_objects_reject_before_engine() -> io::Result<()> {
    let source = SourceFixture::new("environment-match");
    let input = PrivateInput::new("MODE", Some("on"))?;
    let policy = CommandH1HostPolicy::environment("MODE").expect("root approval");
    let prepared = source.prepare(Some(&input), &policy).expect("real preparation");
    let substitute = CommandH1HostPolicy::environment("MODE").expect("new equal-key object");
    let error = prepared.execute(&substitute).err().expect("policy identity substitution rejects");
    assert_eq!(error[0].code(), "ZRYNA-C4102");
    input.assert_released()?;
    let prepared = source.prepare(Some(&input), &policy).expect("original policy recovery");
    let clone = policy.clone();
    clone.revoke();
    let error = prepared.execute(&policy).err().expect("shared monotonic revocation rejects");
    assert_eq!(error[0].code(), "ZRYNA-C4102");
    rejected_preparation(&source, Some(&input), &policy, "ZRYNA-C4102");
    input.assert_released()?;
    let policy = CommandH1HostPolicy::environment("MODE").expect("fresh root approval");
    let run = source
        .prepare(Some(&input), &policy)
        .expect("fresh approved preparation")
        .execute(&policy.clone())
        .expect("clone retains exact approval identity");
    returned(&run, CommandH1RunReturn::Ok);
    drop(run);
    input.assert_released()?;
    Ok(())
}

#[test]
fn actual_file_privacy_revocation_between_prepare_and_execute_rejects() -> io::Result<()> {
    let source = SourceFixture::new("environment-match");
    let input = PrivateInput::new("MODE", Some("on"))?;
    let policy = CommandH1HostPolicy::environment("MODE").expect("root approval");
    let prepared = source.prepare(Some(&input), &policy).expect("private retained preparation");
    input.privacy(true)?;
    let error = prepared.execute(&policy).err().expect("same-handle privacy change rejects");
    assert_eq!(error[0].code(), "ZRYNA-D4101");
    input.assert_released()?;
    input.privacy(false)?;
    let run = source
        .prepare(Some(&input), &policy)
        .expect("fresh captured privacy recovery")
        .execute(&policy)
        .expect("fresh actual execution");
    returned(&run, CommandH1RunReturn::Ok);
    drop(run);
    input.assert_released()?;
    Ok(())
}
