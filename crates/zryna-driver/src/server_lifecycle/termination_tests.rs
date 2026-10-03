//! Deterministic stop/publication interleavings and destructor reentrancy.

use super::{Error, Shared};
use crate::{Resource, attach, input, limits, server_lifecycle::Server};
use std::{
    sync::{
        Arc, Weak,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

struct ReleaseOnDrop(Option<mpsc::Sender<()>>);

impl ReleaseOnDrop {
    fn release(mut self) {
        self.0.take().expect("release sender").send(()).expect("release receiver");
    }
}

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

fn pause_at_stop(shared: &Shared) -> (mpsc::Receiver<()>, ReleaseOnDrop) {
    let (entered, wait) = mpsc::channel();
    let (release, released) = mpsc::channel();
    *shared.stop_hook.lock().expect("hook mutex") = Some(Box::new(move || {
        entered.send(()).expect("stop boundary observer");
        released.recv_timeout(Duration::from_secs(2)).expect("stop cleanup release");
    }));
    (wait, ReleaseOnDrop(Some(release)))
}

struct FailingDrop;

impl Drop for FailingDrop {
    fn drop(&mut self) {
        panic!("injected resource destructor failure");
    }
}

#[test]
fn failure_stop_atomically_revokes_siblings_before_cleanup() {
    let server = Server::start(crate::Limits { requests: 3, ..limits() }).expect("start");
    let failed = server.admit(&input(b"fail")).expect("failing request");
    failed.retain(FailingDrop).expect("failing resource");
    let sibling = server.admit(&input(b"body")).expect("sibling");
    let finishing = server.admit(&input(b"body")).expect("finishing sibling");
    let sibling_drop = attach(&sibling);
    let finishing_drop = attach(&finishing);
    let shared = Arc::clone(&failed.shared);
    let (stopped, release) = pause_at_stop(&shared);
    let cancel = failed.cancellation();
    let worker = thread::spawn(move || cancel.cancel());
    stopped.recv_timeout(Duration::from_secs(2)).expect("failure stop acquired state lock");

    assert_eq!(sibling.check(), Err(Error::Inactive));
    let mut output = [42; 8];
    assert_eq!(sibling.copy_input(&mut output), Err(Error::Inactive));
    assert_eq!(output, [42; 8]);
    assert_eq!(sibling.route(), Err(Error::Inactive));
    let rejected = Arc::new(AtomicUsize::new(0));
    assert_eq!(sibling.retain(Resource(Arc::clone(&rejected))), Err(Error::Inactive));
    assert_eq!(rejected.load(Ordering::SeqCst), 1);
    {
        let state = shared.state.lock().expect("state mutex remains available");
        assert!(state.stopped);
        assert!(state.entries.is_empty(), "stop must detach all siblings in the same lock");
    }
    assert_eq!(finishing.finish(200, b"late"), Err(Error::Inactive));
    assert!(matches!(server.admit(&input(b"new")), Err(Error::Inactive)));
    assert_eq!(sibling_drop.load(Ordering::SeqCst), 0);
    assert_eq!(finishing_drop.load(Ordering::SeqCst), 0);
    assert_eq!(server.usage(), Ok((2, 44)), "revoked resources still occupy quota");

    release.release();
    assert_eq!(worker.join().expect("canceller joined"), Err(Error::Host));
    assert_eq!(sibling_drop.load(Ordering::SeqCst), 1);
    assert_eq!(finishing_drop.load(Ordering::SeqCst), 1);
    assert_eq!(server.usage(), Ok((0, 0)));
    assert_eq!(server.shutdown(), Err(Error::Host));
}

#[test]
fn finish_cannot_publish_after_stop_wins_during_resource_destruction() {
    struct BlockingDrop {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
        destroyed: Arc<AtomicUsize>,
    }
    impl Drop for BlockingDrop {
        fn drop(&mut self) {
            self.entered.send(()).expect("destruction observer");
            self.release.recv_timeout(Duration::from_secs(2)).expect("destruction release");
            self.destroyed.fetch_add(1, Ordering::SeqCst);
        }
    }
    let server = Server::start(limits()).expect("start");
    let request = server.admit(&input(b"body")).expect("request");
    let shared = Arc::clone(&request.shared);
    let destroyed = Arc::new(AtomicUsize::new(0));
    let (entered, destruction) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let destruction_release = ReleaseOnDrop(Some(release));
    request
        .retain(BlockingDrop { entered, release: released, destroyed: Arc::clone(&destroyed) })
        .expect("resource");
    let finishing = thread::spawn(move || request.finish(200, b"late"));
    destruction.recv_timeout(Duration::from_secs(2)).expect("response cleanup began");
    let (stopped, stop_release) = pause_at_stop(&shared);
    let shutdown = thread::spawn(move || server.shutdown());
    stopped.recv_timeout(Duration::from_secs(2)).expect("shutdown acquired state lock");

    destruction_release.release();
    assert_eq!(finishing.join().expect("finisher joined"), Err(Error::Inactive));
    assert_eq!(destroyed.load(Ordering::SeqCst), 1);
    {
        let state = shared.state.lock().expect("state mutex");
        assert!(state.stopped);
        assert_eq!((state.live, state.bytes), (0, 0));
    }
    stop_release.release();
    assert_eq!(shutdown.join().expect("shutdown joined"), Ok(()));
}

#[test]
fn failure_cleanup_allows_reentrant_revocation_without_double_destruction() {
    struct ReentrantDrop {
        shared: Weak<Shared>,
        sibling: u64,
        destroyed: Arc<AtomicUsize>,
    }
    impl Drop for ReentrantDrop {
        fn drop(&mut self) {
            let shared = self.shared.upgrade().expect("host still owns retirement");
            {
                let state = shared.state.try_lock().expect("destructor must run outside mutex");
                assert!(state.stopped);
                assert!(state.entries.is_empty());
            }
            shared.remove(self.sibling).expect("reentrant revocation is idempotent");
            shared.stop();
            self.destroyed.fetch_add(1, Ordering::SeqCst);
        }
    }
    let mut server = Server::start(crate::Limits { requests: 3, ..limits() }).expect("start");
    let failed = server.admit(&input(b"fail")).expect("failing request");
    failed.retain(FailingDrop).expect("resource");
    let reentrant = server.admit(&input(b"body")).expect("reentrant request");
    let sibling = server.admit(&input(b"body")).expect("sibling");
    let reentrant_drop = Arc::new(AtomicUsize::new(0));
    let sibling_drop = attach(&sibling);
    reentrant
        .retain(ReentrantDrop {
            shared: Arc::downgrade(&failed.shared),
            sibling: sibling.id,
            destroyed: Arc::clone(&reentrant_drop),
        })
        .expect("reentrant resource");
    // The stopped worker must finish before try_lock can diagnose a lock held by the
    // retirement caller, rather than legitimate transient contention from that worker.
    let worker = server.worker.take().expect("owned deadline worker");
    *failed.shared.stop_hook.lock().expect("hook mutex") = Some(Box::new(move || {
        assert_eq!(worker.join().expect("stopped deadline worker joined"), Ok(()));
    }));
    assert_eq!(failed.cancellation().cancel(), Err(Error::Host));
    assert_eq!(reentrant_drop.load(Ordering::SeqCst), 1);
    assert_eq!(sibling_drop.load(Ordering::SeqCst), 1);
    reentrant.cancellation().cancel().expect("repeated cancel");
    sibling.cancellation().cancel().expect("repeated sibling cancel");
    assert_eq!(server.usage(), Ok((0, 0)));
    assert_eq!(server.shutdown(), Err(Error::Host));
}
