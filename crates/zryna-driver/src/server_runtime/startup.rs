//! Non-panicking bounded hook for the execution-input startup reservation race.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender},
    },
    time::Duration,
};

use super::Error;

pub(super) struct StagePause {
    pub(super) claimed: AtomicBool,
    pub(super) entered: SyncSender<()>,
    pub(super) release: Mutex<Receiver<()>>,
}

impl StagePause {
    pub(super) fn wait(&self) -> Result<(), Error> {
        if self.claimed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        self.entered.send(()).map_err(|_| Error::Host)?;
        self.release
            .lock()
            .map_err(|_| Error::Host)?
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| Error::Host)
    }
}
