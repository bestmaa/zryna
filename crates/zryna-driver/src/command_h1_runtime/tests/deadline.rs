//! Actual engine epoch expiry and unwinding are separate from ordinary fuel traps.

use super::*;

#[test]
fn actual_epoch_deadline_traps_with_no_identity_and_confirmed_teardown() {
    let source = SourceFixture::new("pure-entry");
    let policy = CommandH1HostPolicy::deny_all();
    let run = source
        .prepare(None, &policy)
        .expect("real pure preparation")
        .execute_before_call(&policy, &|| {
            std::thread::sleep(envelope::DEADLINE + std::time::Duration::from_millis(100));
        })
        .expect("actual bounded session");
    assert_eq!(
        run.record().outcome(),
        &CommandH1Outcome::RuntimeTrap {
            category: CommandH1TrapCategory::HostProcessFailure,
            identity: None,
        }
    );
    assert_eq!(run.record().teardown(), CommandH1Teardown::Confirmed);
    assert!(!run.record().succeeded());
    drop(run);
    fresh_recovery();
}

#[test]
fn actual_host_unwind_has_no_run_or_identity_and_never_confirms_teardown() {
    let source = SourceFixture::new("pure-entry");
    let policy = CommandH1HostPolicy::deny_all();
    let run = source
        .prepare(None, &policy)
        .expect("real pure preparation")
        .execute_before_call(&policy, &|| panic!("fixed host unwind fixture"))
        .expect("caught host exception is a consumed execution observation");
    assert_eq!(
        run.record().outcome(),
        &CommandH1Outcome::RuntimeTrap {
            category: CommandH1TrapCategory::HostProcessFailure,
            identity: None,
        }
    );
    assert_eq!(run.record().teardown(), CommandH1Teardown::Unconfirmed);
    assert!(!run.record().succeeded());
    drop(run);
    fresh_recovery();
}
