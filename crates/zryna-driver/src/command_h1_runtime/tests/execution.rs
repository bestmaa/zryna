use super::*;
use std::io;

#[test]
fn actual_pure_owned_control_flow_and_weak_upgrade_runs_return_ok() {
    for name in ["pure-entry", "owned-aggregates", "control-flow", "weak-upgrade"] {
        let source = SourceFixture::new(name);
        let policy = CommandH1HostPolicy::deny_all();
        let prepared = source.prepare(None, &policy).expect("real source lowering and factory");
        assert_eq!(source.provider.calls.load(Ordering::SeqCst), 1);
        assert!(prepared.diagnostics().is_empty());
        let binding = *prepared.artifact().program_binding();
        let run = prepared.execute(&policy).expect("actual fresh consuming component run");
        returned(&run, CommandH1RunReturn::Ok);
        assert_eq!(run.artifact().program_binding(), &binding);
    }
}

#[test]
fn actual_pure_false_is_a_declared_wit_error_with_confirmed_teardown() {
    let source = SourceFixture::new("pure-false");
    let policy = CommandH1HostPolicy::deny_all();
    let run = source
        .prepare(None, &policy)
        .expect("verified false source")
        .execute(&policy)
        .expect("actual false run");
    returned(&run, CommandH1RunReturn::Err);
    fresh_recovery();
}

#[test]
fn actual_found_empty_ascii_multibyte_and_exact_1024_bytes_run_ok() -> io::Result<()> {
    let source = SourceFixture::new("environment-match");
    for value in [String::new(), "on".into(), "é🙂".into(), "a".repeat(1024), "🙂".repeat(256)]
    {
        assert!(value.len() <= 1024);
        let input = PrivateInput::new("MODE", Some(&value))?;
        let policy = CommandH1HostPolicy::environment("MODE").expect("explicit root approval");
        let prepared = source.prepare(Some(&input), &policy).expect("private captured input");
        #[cfg(windows)]
        input.assert_retained();
        let run = prepared.execute(&policy).expect("actual environment component execution");
        returned(&run, CommandH1RunReturn::Ok);
        #[cfg(windows)]
        input.assert_retained();
        drop(run);
        input.assert_released()?;
    }
    fresh_recovery();
    Ok(())
}

#[test]
fn actual_missing_and_helper_calls_preserve_declared_found_missing_results() -> io::Result<()> {
    for name in [
        "environment-match",
        "environment-helper",
        "environment-live-prefix",
        "environment-trailing",
    ] {
        for value in [None, Some("")] {
            let source = SourceFixture::new(name);
            let input = PrivateInput::new("MODE", value)?;
            let policy = CommandH1HostPolicy::environment("MODE").expect("explicit key approval");
            let run = source
                .prepare(Some(&input), &policy)
                .expect("real source preparation")
                .execute(&policy)
                .expect("real source execution");
            returned(
                &run,
                if value.is_some() { CommandH1RunReturn::Ok } else { CommandH1RunReturn::Err },
            );
            drop(run);
            input.assert_released()?;
        }
    }
    Ok(())
}

#[test]
fn actual_bounds_trap_has_audited_identity_and_fuel_exhaustion_has_none() {
    let policy = CommandH1HostPolicy::deny_all();
    let source = SourceFixture::new("language-bounds");
    let run = source
        .prepare(None, &policy)
        .expect("real controlled bounds source")
        .execute(&policy)
        .expect("bounds execution observed");
    assert_eq!(
        run.record().outcome(),
        &CommandH1Outcome::RuntimeTrap {
            category: CommandH1TrapCategory::ControlledLanguage,
            identity: Some("zryna.trap.bounds-v1".into()),
        }
    );
    assert_eq!(run.record().teardown(), CommandH1Teardown::Confirmed);
    assert!(!run.record().succeeded());
    drop(run);
    fresh_recovery();
    let source = SourceFixture::new("fuel-exhaustion");
    let run = source
        .prepare(None, &policy)
        .expect("real unbounded source loop")
        .execute(&policy)
        .expect("finite runtime envelope observed");
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
fn actual_foreign_store_callback_rejects_without_value_or_false_policy_denial() -> io::Result<()> {
    use super::super::{envelope, host};
    use std::sync::Arc;
    use wasmtime::{
        Store,
        component::{Component, Val},
    };

    let source = SourceFixture::new("environment-match");
    let input = PrivateInput::new("MODE", Some("on"))?;
    let policy = CommandH1HostPolicy::environment("MODE").expect("root approval");
    let prepared = source.prepare(Some(&input), &policy).expect("real sealed preparation");
    let engine = envelope::engine().expect("actual fixed runtime engine");
    let component = Component::new(&engine, prepared.artifact().bytes())
        .expect("actual retained component compilation");
    let owner = host::Host::new(Arc::clone(&prepared.authority));
    let linker = host::linker(&engine, &component, &owner).expect("linker sealed for Host A");
    let foreign = host::Host::new(Arc::clone(&prepared.authority));
    let mut store = Store::new(&engine, foreign);
    store.limiter(|host| &mut host.limits);
    store.set_fuel(envelope::FUEL).expect("bounded execution fuel");
    store.set_epoch_deadline(1);
    let deadline = envelope::Deadline::start(&engine).expect("owned deadline worker");
    let instance = linker
        .instantiate(&mut store, &component)
        .expect("valid component instantiation precedes mediated callback");
    let interface = instance
        .get_export_index(&mut store, None, "wasi:cli/run@0.2.12")
        .expect("audited run interface");
    let export =
        instance.get_export_index(&mut store, Some(&interface), "run").expect("audited run export");
    let run = instance
        .get_typed_func::<(), (Result<(), ()>,)>(&mut store, &export)
        .expect("actual canonical run type");
    let error = run.call(&mut store, ()).expect_err("foreign Host B callback rejects");
    assert_eq!(error.root_cause().to_string(), "command host store authority changed");
    assert!(store.data().denial.is_none(), "store substitution is not root policy denial");
    let mut output = [Val::Bool(true)];
    assert!(
        store
            .data_mut()
            .call("wasi:cli/environment@0.2.12", "get-environment", &[], &mut output)
            .is_err()
    );
    assert!(matches!(&output[0], Val::Bool(true)), "consumed host publishes no input value");
    assert!(store.data().denial.is_none());
    drop(store);
    assert!(deadline.finish(), "foreign store's owned deadline worker joined");
    drop(owner);
    let run = prepared.execute(&policy).expect("fresh legitimate host and linker recover");
    returned(&run, CommandH1RunReturn::Ok);
    drop(run);
    input.assert_released()?;
    Ok(())
}
