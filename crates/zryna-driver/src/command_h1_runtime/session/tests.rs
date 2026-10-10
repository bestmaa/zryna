//! The production classifier sees a real canonical guard frame from a malformed test host.

use super::*;
use crate::command_h1_runtime::{
    CommandH1HostPolicy,
    tests::{SourceFixture, private_input::PrivateInput},
};
use wasmtime::component::Val;

#[test]
fn actual_canonical_guard_classification_authenticates_component_image_and_sealed_storage_sites()
-> std::io::Result<()> {
    let source = SourceFixture::new("environment-match");
    let input = PrivateInput::new("MODE", Some("on"))?;
    let policy = CommandH1HostPolicy::environment("MODE").expect("exact approval");
    let prepared = source.prepare(Some(&input), &policy).expect("actual admitted preparation");
    let engine = envelope::engine().expect("fixed engine");
    let component = Component::new(&engine, prepared.artifact().bytes()).expect("sealed component");
    let state = host::Host::new(Arc::clone(&prepared.authority));
    let mut linker = host::linker(&engine, &component, &state).expect("exact sealed host linker");
    // Only this test violates the host-result contract. The production host never accepts an
    // over-1024-byte private input or exposes a mutable linker to callers.
    linker.allow_shadowing(true);
    linker
        .instance("wasi:cli/environment@0.2.12")
        .expect("exact environment instance")
        .func_new("get-environment", |_, _, parameters, results| {
            assert!(parameters.is_empty());
            assert_eq!(results.len(), 1);
            results[0] = Val::List(vec![Val::Tuple(vec![
                Val::String("MODE".into()),
                Val::String("x".repeat(1025)),
            ])]);
            Ok(())
        })
        .expect("test-only malformed host override");
    let mut store = Store::new(&engine, state);
    store.limiter(|state| &mut state.limits);
    store.set_fuel(envelope::FUEL).expect("fixed fuel");
    store.set_epoch_deadline(1);
    let deadline = envelope::Deadline::start(&engine).expect("owned deadline");
    let instance = linker.instantiate(&mut store, &component).expect("real sealed graph");
    let interface =
        instance.get_export_index(&mut store, None, "wasi:cli/run@0.2.12").expect("run");
    let export = instance.get_export_index(&mut store, Some(&interface), "run").expect("function");
    let run =
        instance.get_typed_func::<(), (Result<(), ()>,)>(&mut store, &export).expect("WIT type");
    let error = run.call(&mut store, ()).expect_err("canonical transfer guard fails fatally");
    assert!(store.data().denial.is_none(), "canonical failure is not host policy denial");
    assert_eq!(
        classify(&error, &component, &prepared.authority),
        CommandH1Outcome::RuntimeTrap {
            category: CommandH1TrapCategory::InterfaceViolation,
            identity: Some("zryna.command.interface-violation.v1".into()),
        }
    );
    let pure = SourceFixture::new("pure-entry");
    let foreign =
        pure.prepare(None, &CommandH1HostPolicy::deny_all()).expect("valid foreign authority");
    let foreign_component =
        Component::new(&engine, foreign.artifact().bytes()).expect("different compiled image");
    assert_eq!(classify(&error, &foreign_component, &prepared.authority), process_failure());
    drop(store);
    assert!(deadline.finish(), "actual worker joined after fatal guard");
    let valid = prepared.execute(&policy).expect("fresh legitimate host recovers");
    assert!(valid.record().succeeded());
    drop(valid);
    input.assert_released()?;
    Ok(())
}
