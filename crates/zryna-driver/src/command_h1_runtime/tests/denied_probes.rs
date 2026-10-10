//! Real lower-boundary calls exercise the production host with independently typed probes.
//! These fixtures cannot construct a sealed command artifact or an execution manifest.

mod component;

use super::super::{envelope, host};
use super::{CommandH1HostPolicy, PrivateInput, SourceFixture, fresh_recovery};
use std::sync::Arc;
use wasmtime::{Store, component::Component};

#[test]
fn actual_denied_capability_matrix_seals_first_callback_and_consumes_each_store() {
    let source = SourceFixture::new("pure-entry");
    let policy = CommandH1HostPolicy::deny_all();
    let prepared = source.prepare(None, &policy).expect("authentic empty-grant authority");
    for probe in component::Probe::ALL {
        execute_probe(&prepared.authority, probe);
    }
    fresh_recovery();
}

#[test]
fn actual_environment_grant_does_not_approve_other_capability_callbacks() -> std::io::Result<()> {
    let source = SourceFixture::new("environment-match");
    let input = PrivateInput::new("MODE", Some("private-probe-fixture"))?;
    let policy = CommandH1HostPolicy::environment("MODE").expect("root approval");
    let prepared = source.prepare(Some(&input), &policy).expect("authentic H1 authority");
    for probe in component::Probe::ALL.into_iter().skip(1) {
        execute_probe(&prepared.authority, probe);
    }
    let run = prepared.execute(&policy).expect("fresh legitimate environment callback");
    assert!(run.record().succeeded());
    drop(run);
    input.assert_released()?;
    Ok(())
}

fn execute_probe(authority: &Arc<super::super::Authority>, probe: component::Probe) {
    let bytes = component::bytes(probe);
    wasmparser::Validator::new().validate_all(&bytes).expect("independent typed fixture");
    let engine = envelope::engine().expect("production restricted engine");
    let component = Component::new(&engine, &bytes).expect("bounded probe compilation");
    let state = host::Host::new(Arc::clone(authority));
    let linker = host::linker(&engine, &component, &state).expect("production host callbacks");
    let mut store = Store::new(&engine, state);
    store.limiter(|state| &mut state.limits);
    store.set_fuel(envelope::FUEL).expect("fixed fuel");
    store.set_epoch_deadline(1);
    let deadline = envelope::Deadline::start(&engine).expect("owned deadline worker");
    let instance = linker.instantiate(&mut store, &component).expect("typed instantiation");
    let run = instance
        .get_typed_func::<(), (Result<(), ()>,)>(&mut store, "run")
        .expect("declared canonical result");
    for _ in 0..2 {
        assert!(run.call(&mut store, ()).is_err(), "denied probe cannot return data");
        let denial = store.data().denial.as_ref().expect("actual authenticated callback");
        assert_eq!(denial.interface, probe.interface());
        assert_eq!(denial.operation, probe.operation());
    }
    drop(store);
    assert!(deadline.finish(), "fatal store consumed and worker joined");
}
