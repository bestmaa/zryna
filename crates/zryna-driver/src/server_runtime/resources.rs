//! One bounded store-owned resource registry; identifiers and resource kinds never alias.

use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::Ordering},
};

use super::{Error, observation::Observation};

pub(super) enum Kind {
    Incoming(Incoming),
    Outparam,
    Fields,
    Response { status: u16, body_started: bool, finished: bool },
    Body { response: u32 },
}

/// The execution-owned input copy stays under its retained revoker from creation to destruction.
pub(super) struct Incoming {
    method: String,
    path: String,
    body: Vec<u8>,
    observation: Arc<Observation>,
}

impl Incoming {
    pub(super) fn new(
        input: &crate::server_lifecycle::Input<'_>,
        observation: Arc<Observation>,
    ) -> Self {
        let incoming = Self {
            method: input.method.to_owned(),
            path: input.path.to_owned(),
            body: input.body.to_vec(),
            observation,
        };
        incoming.observation.input_copies.fetch_add(1, Ordering::SeqCst);
        incoming.observation.input_copy_bytes.fetch_add(incoming.bytes(), Ordering::SeqCst);
        incoming
    }

    fn bytes(&self) -> usize {
        self.method.len() + self.path.len() + self.body.len()
    }
}

impl Drop for Incoming {
    fn drop(&mut self) {
        self.observation.input_copies.fetch_sub(1, Ordering::SeqCst);
        self.observation.input_copy_bytes.fetch_sub(self.bytes(), Ordering::SeqCst);
    }
}

pub(super) struct Entry {
    pub(super) kind: Kind,
    pub(super) ty: u32,
    observation: Arc<Observation>,
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.observation.resources.fetch_sub(1, Ordering::SeqCst);
        self.observation.destroyed.fetch_add(1, Ordering::SeqCst);
    }
}

pub(super) struct Resources {
    entries: BTreeMap<u32, Entry>,
    next: u32,
    limit: usize,
    observation: Arc<Observation>,
}

impl Resources {
    pub(super) fn new(limit: usize, observation: Arc<Observation>) -> Self {
        Self { entries: BTreeMap::new(), next: 1, limit, observation }
    }

    pub(super) fn insert(&mut self, kind: Kind, ty: u32) -> Result<u32, Error> {
        if self.entries.len() >= self.limit {
            return Err(Error::Limit);
        }
        let id = self.next;
        self.next = id.checked_add(1).ok_or(Error::Limit)?;
        if let Kind::Incoming(incoming) = &kind {
            self.observation.incoming_bytes.fetch_add(incoming.bytes(), Ordering::SeqCst);
        }
        self.observation.resources.fetch_add(1, Ordering::SeqCst);
        self.observation.created.fetch_add(1, Ordering::SeqCst);
        self.entries.insert(id, Entry { kind, ty, observation: Arc::clone(&self.observation) });
        Ok(id)
    }

    pub(super) fn get_mut(&mut self, id: u32, ty: u32) -> Result<&mut Kind, Error> {
        let entry = self.entries.get_mut(&id).ok_or(Error::Resource)?;
        if entry.ty != ty {
            return Err(Error::Resource);
        }
        Ok(&mut entry.kind)
    }

    pub(super) fn take(&mut self, id: u32, ty: u32) -> Result<Entry, Error> {
        if self.entries.get(&id).is_none_or(|entry| entry.ty != ty) {
            return Err(Error::Resource);
        }
        self.entries.remove(&id).ok_or(Error::Resource)
    }
}
