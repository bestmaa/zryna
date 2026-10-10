//! Shared private lifecycle fixtures; never compiled into production.

use crate::server_lifecycle::{Input, Limits};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

pub(crate) fn limits() -> Limits {
    Limits {
        requests: 2,
        request_bytes: 8,
        response_bytes: 8,
        buffer_bytes: 128,
        timeout: Duration::from_secs(5),
    }
}
pub(crate) fn input(body: &[u8]) -> Input<'_> {
    Input {
        method: "POST",
        path: "/local",
        body,
        deadline: Instant::now() + Duration::from_secs(4),
    }
}
pub(crate) struct Resource(pub(crate) Arc<AtomicUsize>);
impl Drop for Resource {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
pub(crate) fn attach(request: &crate::server_lifecycle::Request) -> Arc<AtomicUsize> {
    let destroyed = Arc::new(AtomicUsize::new(0));
    request.retain(Resource(Arc::clone(&destroyed))).expect("retain resource");
    destroyed
}
