//! Internal exact-world execution/denial/teardown evidence; no public selector or listener.
#![forbid(unsafe_code)]

#[path = "../server_lifecycle/mod.rs"]
mod server_lifecycle;
#[path = "mod.rs"]
mod server_runtime;
#[path = "../server_transport/mod.rs"]
mod server_transport;

// The lifecycle module includes its three existing termination regression cases. Their shared
// fixtures are retained here without editing the separately owned lifecycle target.
use server_lifecycle::{Input, Limits};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

fn limits() -> Limits {
    Limits {
        requests: 2,
        request_bytes: 8,
        response_bytes: 8,
        buffer_bytes: 128,
        timeout: Duration::from_secs(5),
    }
}
fn input(body: &[u8]) -> Input<'_> {
    Input {
        method: "POST",
        path: "/local",
        body,
        deadline: Instant::now() + Duration::from_secs(4),
    }
}
struct Resource(Arc<AtomicUsize>);
impl Drop for Resource {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
fn attach(request: &server_lifecycle::Request) -> Arc<AtomicUsize> {
    let destroyed = Arc::new(AtomicUsize::new(0));
    request.retain(Resource(Arc::clone(&destroyed))).expect("retain resource");
    destroyed
}
