use std::{
    sync::{Mutex, atomic::AtomicBool, mpsc},
    thread,
    time::{Duration, Instant},
};

use super::super::startup::StagePause;
use super::*;

struct Release(Option<mpsc::Sender<()>>);
impl Drop for Release {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

#[test]
fn deadline_cannot_release_reservation_while_execution_input_is_staged() {
    let observation = Arc::new(Observation::default());
    let limits =
        Envelope { requests: crate::Limits { requests: 1, ..crate::limits() }, ..envelope() };
    let mut prepared =
        prepare("201", ServerOperation::Reply, EMPTY, Approval::deny_all(), limits, &observation)
            .expect("prepare");
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, released) = mpsc::channel();
    prepared.stage_pause = Some(Arc::new(StagePause {
        claimed: AtomicBool::new(false),
        entered,
        release: Mutex::new(released),
    }));
    let server = Arc::new(Server::start(prepared).expect("start"));
    let release = Release(Some(release));
    let admitting = Arc::clone(&server);
    let admission = thread::spawn(move || {
        let input = crate::Input {
            deadline: Instant::now() + Duration::from_millis(100),
            ..crate::input(b"ok")
        };
        admitting.admit(&input)
    });
    waiting.recv_timeout(Duration::from_secs(2)).expect("actual owned input was staged");
    let until = Instant::now() + Duration::from_secs(1);
    while observation.revocations.load(Ordering::SeqCst) == 0 && Instant::now() < until {
        thread::yield_now();
    }
    assert_eq!(
        observation.revocations.load(Ordering::SeqCst),
        1,
        "autonomous deadline worker entered the retained revoker"
    );
    assert_eq!(observation.input_copies.load(Ordering::SeqCst), 1);
    assert_eq!(observation.input_copy_bytes.load(Ordering::SeqCst), 12);
    assert_eq!(observation.stores_created.load(Ordering::SeqCst), 0);
    assert_eq!(server.usage().expect("still reserved").0, 1);
    assert!(matches!(
        server.admit(&crate::input(b"new")),
        Err(Error::Lifecycle(crate::server_lifecycle::Error::Limit))
    ));
    drop(release);
    let pending = admission.join().expect("admission joined").expect("accepted before expiry");
    assert!(pending.wait().is_err());
    clean(&observation);
    assert_eq!(
        observation.stores_created.load(Ordering::SeqCst),
        0,
        "an expired staged request cannot create a guest Store"
    );
    let server = Arc::try_unwrap(server).ok().expect("sole host owner");
    // Expiry owns the detached lifecycle entry. Guest/input teardown precedes its final
    // bookkeeping; observe that owner complete retirement before testing quota recovery.
    let until = Instant::now() + Duration::from_secs(1);
    while server.usage().expect("retiring reservation").0 != 0 && Instant::now() < until {
        thread::yield_now();
    }
    assert_eq!(server.usage(), Ok((0, 0)));
    assert_eq!(server.admit(&crate::input(b"new")).expect("recovery").wait(), Ok((201, vec![])));
    server.shutdown().expect("joined");
    clean(&observation);
}
