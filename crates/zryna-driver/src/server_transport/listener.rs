//! Serial loopback admission: bounded attempts and service time include malformed clients.

use super::{
    Config, Error, framing,
    io::{self, Control, Observation, Socket},
};
use crate::{
    server_lifecycle::Input,
    server_runtime::{Prepared, Server},
};
use std::{
    net::{SocketAddr, TcpListener},
    sync::{Arc, atomic::Ordering},
    time::Instant,
};

pub(super) struct Bound {
    listener: Option<TcpListener>,
    runtime: Option<Server>,
    config: Config,
    control: Arc<Control>,
    observation: Arc<Observation>,
}

impl Bound {
    pub(super) fn start(
        mut config: Config,
        prepared: Prepared,
        observation: Arc<Observation>,
    ) -> Result<Self, Error> {
        config = config.validate()?;
        // Configuration and source binding must agree before either lifecycle or listener starts.
        if config.envelope().identity() != prepared.envelope.identity() {
            return Err(Error::Config);
        }
        let runtime = Server::start(prepared)?;
        let listener = TcpListener::bind(config.address).map_err(|_| Error::Io)?;
        listener.set_nonblocking(true).map_err(|_| Error::Io)?;
        config.address = listener.local_addr().map_err(|_| Error::Io)?;
        observation.listeners.fetch_add(1, Ordering::SeqCst);
        Ok(Self {
            listener: Some(listener),
            runtime: Some(runtime),
            config,
            control: Arc::new(Control::default()),
            observation,
        })
    }
    pub(super) fn address(&self) -> SocketAddr {
        self.config.address
    }
    pub(super) fn control(&self) -> Arc<Control> {
        Arc::clone(&self.control)
    }
    pub(super) fn run(mut self) -> Result<(), Error> {
        let deadline = Instant::now() + self.config.service_timeout;
        let result = self.serve(deadline);
        self.terminate()?;
        result
    }
    fn serve(&mut self, deadline: Instant) -> Result<(), Error> {
        let mut attempts = 0;
        while attempts < self.config.attempts
            && Instant::now() < deadline
            && !self.control.stopped.load(Ordering::SeqCst)
        {
            let stream = match self.listener.as_ref().ok_or(Error::Stopped)?.accept() {
                Ok((stream, peer)) if peer.ip().is_loopback() => stream,
                Ok((stream, _)) => {
                    drop(stream);
                    return Err(Error::Io);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    io::pause();
                    continue;
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(Error::Io),
            };
            attempts += 1;
            self.observation.accepted.fetch_add(1, Ordering::SeqCst);
            let socket = Socket::new(
                stream,
                Arc::clone(&self.control),
                Arc::clone(&self.observation),
                self.config.buffer_reservation(),
                deadline.min(Instant::now() + self.config.request_timeout),
            )?;
            match self.request(socket) {
                Ok(()) => {
                    self.observation.served.fetch_add(1, Ordering::SeqCst);
                }
                Err(_) => {
                    self.observation.rejected.fetch_add(1, Ordering::SeqCst);
                }
            }
            // A replacement request is impossible until all transport objects are destroyed.
            if self.observation.socket_handles.load(Ordering::SeqCst) != 0
                || self.observation.reserved_bytes.load(Ordering::SeqCst) != 0
            {
                return Err(Error::Io);
            }
        }
        Ok(())
    }
    fn request(&self, mut socket: Socket) -> Result<(), Error> {
        let decoded = framing::read(&mut socket, self.config)?;
        let input = Input {
            method: &decoded.method,
            path: &decoded.path,
            body: &decoded.body,
            deadline: socket.deadline,
        };
        let pending = self.runtime.as_ref().ok_or(Error::Stopped)?.admit(&input)?;
        // The runtime owns its two charged copies; discard the transport's decoded copy now.
        drop(decoded);
        let cancel = pending.cancellation();
        self.control.register(move || {
            let _ = cancel();
        });
        pending.publish(move |status| {
            socket.respond(status).map_err(|error| match error {
                Error::Deadline => crate::server_lifecycle::Error::Deadline,
                Error::Stopped => crate::server_lifecycle::Error::Inactive,
                _ => crate::server_lifecycle::Error::Host,
            })
        })?;
        Ok(())
    }
    fn terminate(&mut self) -> Result<(), Error> {
        self.control.stop();
        if self.listener.take().is_some() {
            self.observation.listeners.fetch_sub(1, Ordering::SeqCst);
        }
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown()?;
        }
        Ok(())
    }
}
impl Drop for Bound {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}
