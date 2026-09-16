//! Bounded channel lifetime and shutdown; never owns runtime children or leases.
use super::{
    codec,
    dto::{Complete, Event, Hello},
    pipe_io as io,
    state::{Shared, State},
    worker,
};
use std::io as stdio;
use std::os::fd::AsRawFd;
use std::sync::{
    mpsc::{sync_channel, SyncSender},
    Arc, Mutex,
};
use std::thread::JoinHandle;
use std::time::Instant;
pub(super) struct Channel {
    sender: SyncSender<Event>,
    pub state: Shared,
    thread: Option<JoinHandle<()>>,
}
impl Channel {
    pub fn open(digest: &str, key_required: bool) -> stdio::Result<(Self, Hello)> {
        let (input, output) = io::pipes()?;
        let started = Instant::now();
        let mut reader = io::Reader::new();
        let hello = loop {
            if started.elapsed() >= io::IO_BOUND || super::signals::requested() {
                return Err(codec::invalid());
            }
            match reader.step(input.as_raw_fd())? {
                io::ReadResult::Frame(mut bytes) => {
                    let value = codec::hello(&bytes, digest, key_required);
                    bytes.fill(0);
                    if started.elapsed() >= io::IO_BOUND {
                        return Err(codec::invalid());
                    }
                    break value?;
                }
                io::ReadResult::Eof => return Err(codec::invalid()),
                io::ReadResult::Pending => io::tick(input.as_raw_fd(), output.as_raw_fd(), false)?,
            }
        };
        let state = Arc::new(Mutex::new(State::default()));
        // 31 queued frames plus the reactor's one in-flight frame = at most32.
        let (sender, receiver) = sync_channel(31);
        let shared = state.clone();
        let writer = sender.clone();
        let token = hello.run_token.clone();
        let thread = std::thread::Builder::new()
            .name("owned-runtime-pipes".into())
            .spawn(move || worker::run(input, output, token, shared, receiver, writer))?;
        Ok((
            Self {
                sender,
                state,
                thread: Some(thread),
            },
            hello,
        ))
    }
    pub fn emit(&self, event: Event) {
        if self.sender.try_send(event).is_err() {
            self.state.lock().unwrap().broken();
        }
    }
    pub fn requested(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        if super::signals::requested() {
            state.latch("signal");
        }
        state.cause.is_some()
    }
    pub fn finish(&mut self, mut complete: Complete) -> bool {
        {
            let mut state = self.state.lock().unwrap();
            state.finishing = Some(Instant::now());
            if let Some(cause) = state.cause {
                complete.cause = cause.into();
                if complete.failure_code.is_none() {
                    complete.outcome = "cancelled".into();
                }
            }
            complete.operation_id.clone_from(&state.operation);
            let mut event = Event::new("complete");
            event.complete = Some(complete);
            if self.sender.try_send(event).is_err() {
                state.broken();
            }
        }
        if self
            .thread
            .take()
            .is_some_and(|thread| thread.join().is_err())
        {
            self.state.lock().unwrap().broken();
        }
        !self.state.lock().unwrap().broken
    }
}
impl Drop for Channel {
    fn drop(&mut self) {
        self.state.lock().unwrap().broken();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
