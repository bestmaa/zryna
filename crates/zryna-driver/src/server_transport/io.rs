//! Interruptible nonblocking socket ownership with absolute, nonrenewable deadlines.

use super::Error;
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

type Cancel = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
pub(super) struct Observation {
    pub(super) listeners: AtomicUsize,
    pub(super) socket_handles: AtomicUsize,
    pub(super) reserved_bytes: AtomicUsize,
    pub(super) accepted: AtomicUsize,
    pub(super) served: AtomicUsize,
    pub(super) rejected: AtomicUsize,
}

#[derive(Default)]
pub(super) struct Control {
    pub(super) stopped: AtomicBool,
    interrupt: Mutex<Option<TcpStream>>,
    cancellation: Mutex<Option<Cancel>>,
}
impl Control {
    pub(super) fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(socket) = self.interrupt.lock().expect("interrupt mutex").as_ref() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        let cancellation = self.cancellation.lock().expect("cancellation mutex").clone();
        if let Some(cancel) = cancellation {
            cancel();
        }
    }
    pub(super) fn register(&self, cancel: impl Fn() + Send + Sync + 'static) {
        let cancel: Cancel = Arc::new(cancel);
        *self.cancellation.lock().expect("cancellation mutex") = Some(Arc::clone(&cancel));
        if self.stopped.load(Ordering::SeqCst) {
            cancel();
        }
    }
    fn clear(&self) {
        self.cancellation.lock().expect("cancellation mutex").take();
        self.interrupt.lock().expect("interrupt mutex").take();
    }
}

pub(super) struct Socket {
    stream: Option<TcpStream>,
    pub(super) control: Arc<Control>,
    observation: Arc<Observation>,
    reservation: usize,
    pub(super) deadline: Instant,
}
impl Socket {
    pub(super) fn new(
        stream: TcpStream,
        control: Arc<Control>,
        observation: Arc<Observation>,
        reservation: usize,
        deadline: Instant,
    ) -> Result<Self, Error> {
        stream.set_nonblocking(true).map_err(|_| Error::Io)?;
        let interrupt = stream.try_clone().map_err(|_| Error::Io)?;
        *control.interrupt.lock().map_err(|_| Error::Io)? = Some(interrupt);
        observation.socket_handles.fetch_add(2, Ordering::SeqCst);
        observation.reserved_bytes.fetch_add(reservation, Ordering::SeqCst);
        Ok(Self { stream: Some(stream), control, observation, reservation, deadline })
    }
    pub(super) fn check(&self) -> Result<(), Error> {
        if self.control.stopped.load(Ordering::SeqCst) {
            return Err(Error::Stopped);
        }
        if Instant::now() >= self.deadline {
            return Err(Error::Deadline);
        }
        Ok(())
    }
    pub(super) fn read(&mut self, bytes: &mut [u8]) -> Result<usize, Error> {
        loop {
            self.check()?;
            match self.stream.as_mut().ok_or(Error::Io)?.read(bytes) {
                Ok(count) => return Ok(count),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => pause(),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return Err(Error::Io),
            }
        }
    }
    pub(super) fn has_extra(&self) -> Result<bool, Error> {
        self.check()?;
        match self.stream.as_ref().ok_or(Error::Io)?.peek(&mut [0]) {
            Ok(count) => Ok(count != 0),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
            Err(_) => Err(Error::Io),
        }
    }
    pub(super) fn respond(mut self, status: u16) -> Result<(), Error> {
        // Status is validated by the lifecycle commit. Fixed empty body and close framing.
        let response =
            format!("HTTP/1.1 {status} Zryna\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let mut bytes = response.as_bytes();
        while !bytes.is_empty() {
            self.check()?;
            match self.stream.as_mut().ok_or(Error::Io)?.write(bytes) {
                Ok(0) => return Err(Error::Io),
                Ok(count) => bytes = &bytes[count..],
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => pause(),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return Err(Error::Io),
            }
        }
        // Socket/interrupt handle and response bytes are gone before the reservation is reused.
        self.close();
        Ok(())
    }
    fn close(&mut self) {
        if let Some(stream) = self.stream.take() {
            let _ = stream.shutdown(Shutdown::Both);
            drop(stream);
            self.control.clear();
            self.observation.socket_handles.fetch_sub(2, Ordering::SeqCst);
            self.observation.reserved_bytes.fetch_sub(self.reservation, Ordering::SeqCst);
        }
    }
}
impl Drop for Socket {
    fn drop(&mut self) {
        self.close();
    }
}
pub(super) fn pause() {
    std::thread::park_timeout(Duration::from_millis(1));
}
