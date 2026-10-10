//! Exercises the transport commit primitive in both dedicated lifecycle/runtime targets.

use crate::server_lifecycle::{Error, Input, Limits, Server};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

struct CallbackState {
    server: Arc<Server>,
    observed: Arc<Mutex<Option<usize>>>,
}
impl Drop for CallbackState {
    fn drop(&mut self) {
        // Reenter only from an unused callback's destructor. It must be outside the registry
        // lock and observe the reservation still charged until all callback state is gone.
        *self.observed.lock().expect("observation") =
            Some(self.server.usage().expect("outside lock").0);
    }
}

#[test]
fn rejected_commit_destroys_unused_callback_before_reusing_lifecycle_quota() {
    let server = Arc::new(
        Server::start(Limits {
            requests: 1,
            request_bytes: 8,
            response_bytes: 8,
            buffer_bytes: 128,
            timeout: Duration::from_secs(1),
        })
        .expect("lifecycle"),
    );
    let request = server
        .admit(&Input {
            method: "GET",
            path: "/commit",
            body: b"",
            deadline: Instant::now() + Duration::from_millis(500),
        })
        .expect("admit");
    let observed = Arc::new(Mutex::new(None));
    let state = CallbackState { server: Arc::clone(&server), observed: Arc::clone(&observed) };
    assert_eq!(
        request.publish(199, move || {
            drop(state);
            panic!("invalid status must not invoke publication");
        }),
        Err(Error::Malformed)
    );
    assert_eq!(*observed.lock().expect("observation"), Some(1));
    assert_eq!(server.usage(), Ok((0, 0)));
    Arc::try_unwrap(server)
        .ok()
        .expect("all callback ownership destroyed")
        .shutdown()
        .expect("joined");
}
