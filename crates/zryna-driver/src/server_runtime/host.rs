//! Store-local capability state. Callbacks never reenter lifecycle retirement.

use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::Ordering},
    time::Instant,
};

use wasmtime::StoreLimits;

use super::{
    Envelope, Error, Observation, execution::Control, grants::Grants, resources::Resources,
};

pub(super) struct Host {
    pub(super) limits: StoreLimits,
    pub(super) resources: Resources,
    pub(super) types: BTreeMap<String, u32>,
    pub(super) response: Option<u16>,
    pub(super) observation: Arc<Observation>,
    pub(super) control: Arc<Control>,
    grants: Grants,
    remaining_callbacks: u32,
    remaining_clock_reads: u32,
    pub(super) fatal: Option<Error>,
    clock_origin: Instant,
    store_started: bool,
    #[cfg(test)]
    pub(super) pause: Option<Arc<Pause>>,
}

impl Host {
    pub(super) fn new(
        envelope: Envelope,
        grants: Grants,
        observation: Arc<Observation>,
        control: Arc<Control>,
        clock_origin: Instant,
    ) -> Self {
        Self {
            limits: envelope.store_limits(),
            resources: Resources::new(envelope.resources, Arc::clone(&observation)),
            types: BTreeMap::new(),
            response: None,
            observation,
            control,
            grants,
            remaining_callbacks: envelope.callbacks,
            remaining_clock_reads: grants.clock_reads(),
            fatal: None,
            clock_origin,
            store_started: false,
            #[cfg(test)]
            pause: None,
        }
    }

    pub(super) fn enter(&mut self) -> Result<(), Error> {
        if let Some(error) = self.fatal {
            return Err(error);
        }
        self.control.check()?;
        self.remaining_callbacks = self.remaining_callbacks.checked_sub(1).ok_or(Error::Limit)?;
        Ok(())
    }

    pub(super) fn store_started(&mut self) {
        self.store_started = true;
        self.observation.stores.fetch_add(1, Ordering::SeqCst);
        self.observation.stores_created.fetch_add(1, Ordering::SeqCst);
    }

    pub(super) fn deny(&mut self) -> Error {
        self.observation.denials.fetch_add(1, Ordering::SeqCst);
        self.fatal = Some(Error::Denied);
        Error::Denied
    }

    pub(super) fn clock(&mut self) -> Result<u64, Error> {
        if self.grants.clock_reads() == 0 {
            return Err(self.deny());
        }
        self.remaining_clock_reads =
            self.remaining_clock_reads.checked_sub(1).ok_or(Error::Limit)?;
        #[cfg(test)]
        if let Some(pause) = &self.pause {
            pause.wait()?;
        }
        self.control.check()?;
        self.observation.clock_reads.fetch_add(1, Ordering::SeqCst);
        u64::try_from(self.clock_origin.elapsed().as_nanos()).map_err(|_| Error::Limit)
    }

    pub(super) fn ty(&self, name: &str) -> Result<u32, Error> {
        self.types.get(name).copied().ok_or(Error::Resource)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // Registry fields are destroyed immediately after this destructor on the same thread.
        if self.store_started {
            self.observation.stores.fetch_sub(1, Ordering::SeqCst);
            self.observation.stores_destroyed.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
pub(super) struct Pause {
    pub(super) claimed: std::sync::atomic::AtomicBool,
    pub(super) entered: std::sync::mpsc::SyncSender<()>,
    pub(super) release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    pub(super) panic_after_release: bool,
}
#[cfg(test)]
impl Pause {
    fn wait(&self) -> Result<(), Error> {
        if self.claimed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        self.entered.send(()).map_err(|_| Error::Host)?;
        self.release
            .lock()
            .map_err(|_| Error::Host)?
            .recv_timeout(std::time::Duration::from_secs(2))
            .map_err(|_| Error::Host)?;
        assert!(!self.panic_after_release, "injected host callback panic");
        Ok(())
    }
}
