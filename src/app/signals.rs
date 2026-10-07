// SPDX-License-Identifier: 0BSD

//! Signals wake the Wayland loop through a nonblocking self-pipe.

use std::io::{self, Read};
use std::os::unix::net::UnixStream;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use signal_hook::{
    SigId,
    consts::{SIGHUP, SIGINT, SIGTERM},
};

pub(super) struct Signals {
    pub(super) socket: UnixStream,
    reload: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    registrations: Vec<SigId>,
}

impl Signals {
    pub(super) fn new() -> io::Result<Self> {
        let (socket, writer) = UnixStream::pair()?;
        socket.set_nonblocking(true)?;
        writer.set_nonblocking(true)?;
        let mut signals = Self {
            socket,
            reload: Arc::default(),
            stop: Arc::default(),
            registrations: Vec::new(),
        };
        for (signal, flag) in [
            (SIGHUP, &signals.reload),
            (SIGINT, &signals.stop),
            (SIGTERM, &signals.stop),
        ] {
            signals
                .registrations
                .push(signal_hook::flag::register(signal, flag.clone())?);
            signals
                .registrations
                .push(signal_hook::low_level::pipe::register(
                    signal,
                    writer.try_clone()?,
                )?);
        }
        Ok(signals)
    }

    pub(super) fn pending(&mut self) -> io::Result<(bool, bool)> {
        let mut buffer = [0; 128];
        loop {
            match self.socket.read(&mut buffer) {
                Ok(0) => break,
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }
        Ok((
            self.reload.swap(false, Ordering::SeqCst),
            self.stop.swap(false, Ordering::SeqCst),
        ))
    }
}

impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.registrations.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
