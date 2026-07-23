//! The NetBackend host transport abstraction and the in-process loopback backend.

use std::collections::VecDeque;

pub trait NetBackend: Send {
    fn transmit(&mut self, frame: &[u8]);
    fn poll_receive(&mut self) -> Option<Vec<u8>>;
    fn poll_receive_into(&mut self, out: &mut Vec<u8>) -> bool {
        let Some(mut frame) = self.poll_receive() else {
            return false;
        };
        out.clear();
        out.append(&mut frame);
        true
    }
    fn poll_host_sockets(&mut self) {}
    #[cfg(test)]
    fn test_transmitted_frames(&self) -> Option<&[Vec<u8>]> {
        None
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LoopbackTestBackend {
    pub(crate) transmitted: Vec<Vec<u8>>,
    pub(crate) receive: VecDeque<Vec<u8>>,
}

impl NetBackend for Box<dyn NetBackend> {
    fn transmit(&mut self, frame: &[u8]) {
        self.as_mut().transmit(frame);
    }

    fn poll_receive(&mut self) -> Option<Vec<u8>> {
        self.as_mut().poll_receive()
    }

    fn poll_receive_into(&mut self, out: &mut Vec<u8>) -> bool {
        self.as_mut().poll_receive_into(out)
    }

    fn poll_host_sockets(&mut self) {
        self.as_mut().poll_host_sockets();
    }

    #[cfg(test)]
    fn test_transmitted_frames(&self) -> Option<&[Vec<u8>]> {
        self.as_ref().test_transmitted_frames()
    }
}

impl std::fmt::Debug for dyn NetBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetBackend").finish_non_exhaustive()
    }
}

impl LoopbackTestBackend {
    pub fn push_receive(&mut self, frame: impl Into<Vec<u8>>) {
        self.receive.push_back(frame.into());
    }

    pub fn transmitted_frames(&self) -> &[Vec<u8>] {
        &self.transmitted
    }
}

impl NetBackend for LoopbackTestBackend {
    fn transmit(&mut self, frame: &[u8]) {
        self.transmitted.push(frame.to_vec());
    }

    fn poll_receive(&mut self) -> Option<Vec<u8>> {
        self.receive.pop_front()
    }

    fn poll_receive_into(&mut self, out: &mut Vec<u8>) -> bool {
        let Some(mut frame) = self.receive.pop_front() else {
            return false;
        };
        out.clear();
        out.append(&mut frame);
        true
    }

    #[cfg(test)]
    fn test_transmitted_frames(&self) -> Option<&[Vec<u8>]> {
        Some(self.transmitted_frames())
    }
}
