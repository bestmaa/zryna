//! One request, one Store, and a retained revoker that joins actual guest teardown.

use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use wasmtime::{Engine, Store, UpdateDeadline};

use super::{
    Error, Prepared,
    host::Host,
    linker, operations,
    resources::{Incoming, Kind},
};
use crate::server_lifecycle::{Input, Request};

struct State {
    activated: bool,
    done: bool,
    result: Option<Result<u16, Error>>,
    input: Option<Incoming>,
}

pub(super) struct Control {
    revoked: AtomicBool,
    deadline: Instant,
    engine: Engine,
    state: Mutex<State>,
    wake: Condvar,
    worker: Mutex<Option<JoinHandle<()>>>,
    observation: Arc<super::Observation>,
}

impl Control {
    pub(super) fn check(&self) -> Result<(), Error> {
        if self.revoked.load(Ordering::SeqCst) {
            return Err(Error::Inactive);
        }
        if Instant::now() >= self.deadline {
            return Err(Error::Deadline);
        }
        Ok(())
    }

    fn join(&self) -> Result<(), Error> {
        let mut worker_slot = self.worker.lock().map_err(|_| Error::Host)?;
        if let Some(worker) = worker_slot.take() {
            worker.join().map_err(|_| Error::Host)?;
        }
        Ok(())
    }

    fn revoke(&self) -> Result<(), Error> {
        if !self.revoked.swap(true, Ordering::SeqCst) {
            self.observation.revocations.fetch_add(1, Ordering::SeqCst);
        }
        self.engine.increment_epoch();
        self.wake.notify_all();
        let mut state = self.state.lock().map_err(|_| Error::Host)?;
        while !state.done {
            state = self.wake.wait(state).map_err(|_| Error::Host)?;
        }
        drop(state);
        self.join()
    }
}

struct Revoker(Arc<Control>);
impl Drop for Revoker {
    fn drop(&mut self) {
        let _ = self.0.revoke();
    }
}

pub(super) struct Pending {
    request: Request,
    control: Arc<Control>,
}

impl Pending {
    #[cfg(test)]
    pub(super) fn revoked(&self) -> bool {
        self.control.revoked.load(Ordering::SeqCst)
    }
    #[cfg(test)]
    pub(super) fn response_ready(&self) -> Result<bool, Error> {
        let state = self.control.state.lock().map_err(|_| Error::Host)?;
        Ok(state.done && matches!(state.result, Some(Ok(_))))
    }
    pub(super) fn cancellation(&self) -> impl Fn() -> Result<(), Error> + Send + Sync + 'static {
        let cancellation = self.request.cancellation();
        let control = Arc::clone(&self.control);
        move || {
            let cancelled = cancellation.cancel().map_err(Error::from);
            // A second lifecycle caller can find an already-detached Entry while its first
            // retirement is still running. Every runtime caller separately awaits real teardown.
            let joined = control.revoke();
            cancelled.and(joined)
        }
    }

    pub(super) fn wait(self) -> Result<(u16, Vec<u8>), Error> {
        let result = {
            let mut state = self.control.state.lock().map_err(|_| Error::Host)?;
            while !state.done {
                state = self.control.wake.wait(state).map_err(|_| Error::Host)?;
            }
            state.result.ok_or(Error::Host)?
        };
        self.control.join()?;
        let status = match result {
            Ok(status) => status,
            Err(Error::Denied) => {
                // Retire a denied lease only after actual Store teardown and joining. Doing this
                // inside a guest callback would synchronously join the callback's own thread.
                return match self.request.deny_capability() {
                    Err(crate::server_lifecycle::Error::Denied) => Err(Error::Denied),
                    Err(error) => Err(error.into()),
                    Ok(()) => Err(Error::Host),
                };
            }
            Err(error) => return Err(error),
        };
        // The worker has already destroyed all actual guest state. finish orders publication
        // against cancellation/expiry/shutdown once more, after retained-authority destruction.
        Ok((status, self.request.finish(status, &[])?))
    }
}

pub(super) fn start(
    request: Request,
    input: &Input<'_>,
    prepared: &Arc<Prepared>,
) -> Result<Pending, Error> {
    let control = Arc::new(Control {
        revoked: AtomicBool::new(false),
        deadline: input.deadline,
        engine: prepared.engine.clone(),
        state: Mutex::new(State { activated: false, done: false, result: None, input: None }),
        wake: Condvar::new(),
        worker: Mutex::new(None),
        observation: Arc::clone(&prepared.observation),
    });
    let owned = Arc::clone(&control);
    let worker_prepared = Arc::clone(prepared);
    let worker = thread::Builder::new()
        .name("zryna-server-guest".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut state = owned.state.lock().map_err(|_| Error::Host)?;
                while !state.activated && !owned.revoked.load(Ordering::SeqCst) {
                    state = owned.wake.wait(state).map_err(|_| Error::Host)?;
                }
                if let Err(error) = owned.check() {
                    // A revoked startup must destroy its staged bytes before publishing done.
                    state.input.take();
                    return Err(error);
                }
                let incoming = state.input.take().ok_or(Error::Host)?;
                drop(state);
                execute(&worker_prepared, &owned, incoming)
            }))
            .unwrap_or(Err(Error::Host));
            if let Ok(mut state) = owned.state.lock() {
                state.result = Some(result);
                state.done = true;
                owned.wake.notify_all();
            }
        })
        .map_err(|_| Error::Host)?;
    *control.worker.lock().map_err(|_| Error::Host)? = Some(worker);
    // The worker cannot create a Store until its joined revoker is installed in the lifecycle.
    // If installation fails, the argument's Drop revokes and joins this parked worker.
    request.retain(Revoker(Arc::clone(&control)))?;
    {
        let mut state = control.state.lock().map_err(|_| Error::Host)?;
        control.check()?;
        // Input is the same immutable borrow already validated and charged by lifecycle.admit.
        // Install the revoker first, then create copies while holding the startup gate. Never
        // call Request::check/route/copy_input here: expiry can synchronously retire this guard.
        state.input = Some(Incoming::new(input, Arc::clone(&prepared.observation)));
        #[cfg(test)]
        if let Some(pause) = &prepared.stage_pause {
            pause.wait()?;
        }
        state.activated = true;
    }
    control.wake.notify_all();
    Ok(Pending { request, control })
}

fn execute(prepared: &Prepared, control: &Arc<Control>, incoming: Incoming) -> Result<u16, Error> {
    control.check()?;
    let mut state = Host::new(
        prepared.envelope,
        prepared.grants,
        Arc::clone(&prepared.observation),
        Arc::clone(control),
        prepared.monotonic_origin,
    );
    #[cfg(test)]
    {
        state.pause.clone_from(&prepared.pause);
    }
    let linker =
        linker::bind(&prepared.engine, &prepared.component, &mut state).map_err(|_| Error::Host)?;
    let mut store = Store::new(&prepared.engine, state);
    store.data_mut().store_started();
    store.limiter(|host| &mut host.limits);
    #[cfg(test)]
    let fuel = prepared.probe_fuel.unwrap_or(prepared.envelope.fuel);
    #[cfg(not(test))]
    let fuel = prepared.envelope.fuel;
    store.set_fuel(fuel).map_err(|_| Error::Host)?;
    store.set_epoch_deadline(1);
    store.epoch_deadline_callback(|context| {
        context.data().control.check()?;
        context.data().observation.continued_epochs.fetch_add(1, Ordering::SeqCst);
        // Epoch bumps from another request are wakeups, not authority to cancel this Store.
        Ok(UpdateDeadline::Continue(1))
    });
    let result = (|| {
        control.check()?;
        let instance =
            linker.instantiate(&mut store, &prepared.component).map_err(|_| Error::Guest)?;
        let incoming = operations::own(&mut store, "incoming-request", Kind::Incoming(incoming))?;
        let outparam = operations::own(&mut store, "response-outparam", Kind::Outparam)?;
        let interface = instance
            .get_export_index(&mut store, None, "wasi:http/incoming-handler@0.2.12")
            .ok_or(Error::Artifact)?;
        let handle = instance
            .get_export_index(&mut store, Some(&interface), "handle")
            .ok_or(Error::Artifact)?;
        let handle = instance.get_func(&mut store, handle).ok_or(Error::Artifact)?;
        handle.call(&mut store, &[incoming, outparam], &mut []).map_err(|_| {
            store.data().fatal.unwrap_or_else(|| control.check().err().unwrap_or(Error::Guest))
        })?;
        prepared.observation.returned_calls.fetch_add(1, Ordering::SeqCst);
        control.check()?;
        store.data().response.ok_or(Error::Guest)
    })();
    // Drop precedes the done flag, notification, quota release and response publication.
    if let Ok(fuel) = store.get_fuel() {
        prepared.observation.remaining_fuel.store(fuel, Ordering::SeqCst);
    }
    drop(store);
    result
}
