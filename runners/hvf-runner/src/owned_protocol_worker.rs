//! A bounded duplex pipe reactor remains responsive throughout child teardown.
use super::{codec, dto::Event, pipe_io as io, state::Shared};
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::time::Instant;
pub(super) fn run(
    input: OwnedFd,
    output: OwnedFd,
    token: String,
    state: Shared,
    receiver: Receiver<Event>,
    sender: SyncSender<Event>,
) {
    if io::block_sigpipe_for_thread().is_err() {
        state.lock().unwrap().broken();
        return;
    }
    let mut reader = io::Reader::new();
    let mut open_input = true;
    let mut sequence = 0u64;
    let mut count = 0;
    let mut pending: Option<(Vec<u8>, usize, Instant)> = None;
    loop {
        if state.lock().unwrap().broken {
            return;
        }
        for _ in 0..4 {
            if !open_input {
                break;
            }
            match reader.step(input.as_raw_fd()) {
                Ok(io::ReadResult::Pending) => break,
                Ok(io::ReadResult::Eof) => {
                    open_input = false;
                    state.lock().unwrap().latch("ownerEOF");
                    break;
                }
                Ok(io::ReadResult::Frame(mut bytes)) => {
                    count += 1;
                    let decoded = codec::stop(&bytes, &token);
                    bytes.fill(0);
                    match decoded {
                        Ok(stop) if count <= 16 => state.lock().unwrap().stop(stop, &sender),
                        _ => {
                            state.lock().unwrap().broken();
                            return;
                        }
                    }
                }
                Err(_) => {
                    state.lock().unwrap().broken();
                    return;
                }
            }
        }
        let mut queue_empty = false;
        if pending.is_none() {
            match receiver.try_recv() {
                Ok(mut event) => {
                    sequence = match sequence.checked_add(1) {
                        Some(value) => value,
                        None => {
                            state.lock().unwrap().broken();
                            return;
                        }
                    };
                    event.run_token.clone_from(&token);
                    event.runner_pid = std::process::id();
                    event.sequence = sequence;
                    let payload = match codec::encode(&event) {
                        Ok(bytes) => bytes,
                        Err(_) => {
                            state.lock().unwrap().broken();
                            return;
                        }
                    };
                    let mut bytes = (payload.len() as u32).to_be_bytes().to_vec();
                    bytes.extend(payload);
                    pending = Some((bytes, 0, Instant::now()));
                }
                Err(TryRecvError::Empty) => {
                    queue_empty = true;
                }
                Err(TryRecvError::Disconnected) => return,
            }
        }
        if let Some((bytes, offset, started)) = &mut pending {
            if started.elapsed() >= io::IO_BOUND {
                state.lock().unwrap().broken();
                return;
            }
            match io::write(output.as_raw_fd(), &bytes[*offset..]) {
                Ok(Some(count)) => *offset += count,
                Ok(None) => {}
                Err(_) => {
                    state.lock().unwrap().broken();
                    return;
                }
            }
            if started.elapsed() >= io::IO_BOUND {
                state.lock().unwrap().broken();
                return;
            }
            if *offset == bytes.len() {
                pending = None;
            }
        }
        let finished = state.lock().unwrap().finishing;
        if let Some(at) = finished {
            if at.elapsed() >= io::IO_BOUND {
                state.lock().unwrap().broken();
                return;
            }
            if pending.is_none() && queue_empty {
                return;
            }
        }
        if io::tick(
            if open_input { input.as_raw_fd() } else { -1 },
            output.as_raw_fd(),
            pending.is_some(),
        )
        .is_err()
        {
            state.lock().unwrap().broken();
            return;
        }
    }
}
