//! One consuming call and independently recorded store/watchdog teardown.

use super::{
    Authority, CommandH1ExecutionRecord, CommandH1Outcome, CommandH1RunReturn, CommandH1Teardown,
    CommandH1TrapCategory, envelope, host,
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};
use wasmtime::{Store, component::Component};

#[cfg(test)]
mod tests;

pub(super) fn execute(authority: Arc<Authority>) -> CommandH1ExecutionRecord {
    execute_with_before_call(authority, &|| {})
}

pub(super) fn execute_with_before_call(
    authority: Arc<Authority>,
    before_call: &dyn Fn(),
) -> CommandH1ExecutionRecord {
    match catch_unwind(AssertUnwindSafe(move || instantiate(&authority, before_call))) {
        Ok(Ok(record)) => record,
        // Returned errors precede construction of any store or deadline worker.
        Ok(Err(_)) => {
            CommandH1ExecutionRecord::new(process_failure(), CommandH1Teardown::Confirmed)
        }
        Err(_) => CommandH1ExecutionRecord::new(process_failure(), CommandH1Teardown::Unconfirmed),
    }
}

fn instantiate(
    authority: &Arc<Authority>,
    before_call: &dyn Fn(),
) -> wasmtime::Result<CommandH1ExecutionRecord> {
    let engine = envelope::engine()?;
    let component = Component::new(&engine, authority.artifact.bytes())?;
    let host = host::Host::new(Arc::clone(authority));
    let linker = host::linker(&engine, &component, &host)?;
    let mut store = Store::new(&engine, host);
    store.limiter(|host| &mut host.limits);
    let mut deadline = None;
    let result = (|| {
        store.set_fuel(envelope::FUEL)?;
        store.set_epoch_deadline(1);
        deadline = Some(envelope::Deadline::start(&engine)?);
        let instance = linker.instantiate(&mut store, &component)?;
        let interface = instance
            .get_export_index(&mut store, None, "wasi:cli/run@0.2.12")
            .ok_or_else(|| wasmtime::format_err!("audited command run interface changed"))?;
        let export = instance
            .get_export_index(&mut store, Some(&interface), "run")
            .ok_or_else(|| wasmtime::format_err!("audited command run export changed"))?;
        let run = instance.get_typed_func::<(), (Result<(), ()>,)>(&mut store, &export)?;
        before_call();
        run.call(&mut store, ()).map(|(result,)| result)
    })();
    let outcome = if let Some(denial) = store.data_mut().denial.take() {
        CommandH1Outcome::HostDenial { interface: denial.interface, operation: denial.operation }
    } else {
        match result {
            Ok(result) => CommandH1Outcome::RunReturned {
                result: if result.is_ok() {
                    CommandH1RunReturn::Ok
                } else {
                    CommandH1RunReturn::Err
                },
            },
            Err(error) => classify(&error, &component, authority),
        }
    };
    let destroyed = catch_unwind(AssertUnwindSafe(|| drop(store))).is_ok();
    let joined = deadline.is_none_or(envelope::Deadline::finish);
    let teardown = if destroyed && joined {
        CommandH1Teardown::Confirmed
    } else {
        CommandH1Teardown::Unconfirmed
    };
    Ok(CommandH1ExecutionRecord::new(outcome, teardown))
}

fn classify(
    error: &wasmtime::Error,
    component: &Component,
    authority: &Authority,
) -> CommandH1Outcome {
    if error.downcast_ref::<wasmtime::Trap>() != Some(&wasmtime::Trap::UnreachableCodeReached) {
        return process_failure();
    }
    let Some(frame) =
        error.downcast_ref::<wasmtime::WasmBacktrace>().and_then(|trace| trace.frames().first())
    else {
        return process_failure();
    };
    let Some(offset) = frame.module_offset().and_then(|offset| u64::try_from(offset).ok()) else {
        return process_failure();
    };
    if frame.module().image_range() != component.image_range() {
        return process_failure();
    }
    let exports = frame.module().exports().map(|export| export.name()).collect::<Vec<_>>();
    if exports == ["run", "$zryna$observation"] {
        if let Some(site) = authority.artifact.traps().iter().find(|site| {
            site.function_index() == frame.func_index() && site.module_offset() == offset
        }) {
            return CommandH1Outcome::RuntimeTrap {
                category: CommandH1TrapCategory::ControlledLanguage,
                identity: Some(language_identity(site.identity()).to_owned()),
            };
        }
    } else if exports
        == [
            "memory",
            "arena",
            "status",
            "drops",
            "live",
            "peak",
            "references",
            "allocate",
            "copy",
            "realloc",
            "validate",
            "drain",
            "canonical-state",
        ]
        && let Some(site) = authority.artifact.interface_traps().iter().find(|site| {
            site.function_index() == frame.func_index() && site.module_offset() == offset
        })
    {
        return CommandH1Outcome::RuntimeTrap {
            category: CommandH1TrapCategory::InterfaceViolation,
            identity: Some(site.identity().to_owned()),
        };
    }
    process_failure()
}

fn process_failure() -> CommandH1Outcome {
    CommandH1Outcome::RuntimeTrap {
        category: CommandH1TrapCategory::HostProcessFailure,
        identity: None,
    }
}

fn language_identity(identity: zryna_ir::data_ownership_v1::VerifiedTrapIdentity) -> &'static str {
    use zryna_ir::data_ownership_v1::VerifiedTrapIdentity;
    match identity {
        VerifiedTrapIdentity::BoundsV1 => "zryna.trap.bounds-v1",
        VerifiedTrapIdentity::AllocationV1 => "zryna.trap.allocation-v1",
        VerifiedTrapIdentity::CapacityV1 => "zryna.trap.capacity-v1",
        VerifiedTrapIdentity::RefcountV1 => "zryna.trap.refcount-v1",
        VerifiedTrapIdentity::Utf8V1 => "zryna.trap.utf8-v1",
    }
}
