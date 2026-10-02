//! Direct host-unit proofs complement, and do not claim, guest engine execution.

use super::super::host::Host;
use super::*;
use std::{io, sync::Arc};
use wasmtime::component::Val;

const ENVIRONMENT: &str = "wasi:cli/environment@0.2.12";

#[test]
fn host_unit_approved_callback_returns_exact_private_snapshot_without_ambient_input()
-> io::Result<()> {
    let source = SourceFixture::new("environment-match");
    for value in [None, Some(""), Some("on"), Some("é🙂")] {
        let input = PrivateInput::new("MODE", value)?;
        let policy = CommandH1HostPolicy::environment("MODE").expect("root approval");
        let prepared = source.prepare(Some(&input), &policy).expect("real private preparation");
        let mut host = Host::new(Arc::clone(&prepared.authority));
        let mut output = [Val::Bool(true)];
        for _ in 0..2 {
            assert!(host.call(ENVIRONMENT, "get-environment", &[], &mut output).is_ok());
            let Val::List(entries) = &output[0] else { panic!("canonical list result") };
            match value {
                None => assert!(entries.is_empty()),
                Some(expected) => {
                    let [Val::Tuple(fields)] = entries.as_slice() else {
                        panic!("one exact tuple")
                    };
                    let [Val::String(key), Val::String(value)] = fields.as_slice() else {
                        panic!("canonical string pair")
                    };
                    assert_eq!(&**key, "MODE");
                    assert!(value.as_str().eq(expected));
                }
            }
            assert!(host.denial.is_none());
        }
        drop(output);
        drop(host);
        drop(prepared);
        input.assert_released()?;
    }
    Ok(())
}

#[test]
fn host_unit_arguments_and_cwd_are_closed_empty_observations() {
    let source = SourceFixture::new("pure-entry");
    let policy = CommandH1HostPolicy::deny_all();
    let prepared = source.prepare(None, &policy).expect("pure sealed authority");
    let mut host = Host::new(Arc::clone(&prepared.authority));
    let mut output = [Val::Bool(true)];
    assert!(host.call(ENVIRONMENT, "get-arguments", &[], &mut output).is_ok());
    assert!(matches!(&output[0], Val::List(values) if values.is_empty()));
    assert!(host.call(ENVIRONMENT, "initial-cwd", &[], &mut output).is_ok());
    assert!(matches!(&output[0], Val::Option(None)));
    assert!(host.denial.is_none());
}

#[test]
fn host_unit_disallowed_interfaces_seal_first_denial_and_consume_further_callbacks() {
    let source = SourceFixture::new("pure-entry");
    let policy = CommandH1HostPolicy::deny_all();
    let prepared = source.prepare(None, &policy).expect("pure sealed authority");
    for (interface, operation) in [
        ("wasi:filesystem/types@0.2.12", "read"),
        ("wasi:clocks/monotonic-clock@0.2.12", "now"),
        ("wasi:random/random@0.2.12", "get-random-bytes"),
        ("wasi:sockets/tcp@0.2.12", "start-connect"),
        ("wasi:http/outgoing-handler@0.2.12", "handle"),
        ("wasi:cli/exit@0.2.12", "exit"),
    ] {
        let mut host = Host::new(Arc::clone(&prepared.authority));
        let mut output = [Val::Bool(true)];
        assert!(host.call(interface, operation, &[], &mut output).is_err());
        assert!(host.call(ENVIRONMENT, "get-arguments", &[], &mut output).is_err());
        assert!(
            host.call("wasi:random/random@0.2.12", "get-random-u64", &[], &mut output).is_err()
        );
        let denial = host.denial.as_ref().expect("sealed first denial");
        assert_eq!(denial.interface, interface);
        assert_eq!(denial.operation, operation);
        assert!(matches!(&output[0], Val::Bool(true)));
    }
}

#[test]
fn host_unit_revoked_root_approval_denies_before_copying_value() -> io::Result<()> {
    let source = SourceFixture::new("environment-match");
    let input = PrivateInput::new("MODE", Some("on"))?;
    let policy = CommandH1HostPolicy::environment("MODE").expect("root approval");
    let prepared = source.prepare(Some(&input), &policy).expect("real preparation");
    let mut host = Host::new(Arc::clone(&prepared.authority));
    policy.revoke();
    let mut output = [Val::Bool(true)];
    assert!(host.call(ENVIRONMENT, "get-environment", &[], &mut output).is_err());
    assert!(matches!(&output[0], Val::Bool(true)));
    let denial = host.denial.as_ref().expect("revocation sealed as host denial");
    assert_eq!(denial.interface, ENVIRONMENT);
    assert_eq!(denial.operation, "get-environment");
    drop(host);
    drop(prepared);
    input.assert_released()?;
    Ok(())
}

#[test]
fn host_unit_actual_file_privacy_change_denies_before_copying_value() -> io::Result<()> {
    let source = SourceFixture::new("environment-match");
    let input = PrivateInput::new("MODE", Some("on"))?;
    let policy = CommandH1HostPolicy::environment("MODE").expect("root approval");
    let prepared = source.prepare(Some(&input), &policy).expect("real private preparation");
    let mut host = Host::new(Arc::clone(&prepared.authority));
    input.privacy(true)?;
    let mut output = [Val::Bool(true)];
    assert!(host.call(ENVIRONMENT, "get-environment", &[], &mut output).is_err());
    assert!(matches!(&output[0], Val::Bool(true)));
    assert_eq!(host.denial.as_ref().expect("privacy denial").operation, "get-environment");
    drop(host);
    drop(prepared);
    input.privacy(false)?;
    let run = source
        .prepare(Some(&input), &policy)
        .expect("fresh private capture")
        .execute(&policy)
        .expect("fresh actual engine recovery");
    returned(&run, CommandH1RunReturn::Ok);
    drop(run);
    input.assert_released()?;
    Ok(())
}
