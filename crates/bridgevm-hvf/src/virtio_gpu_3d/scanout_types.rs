//! Plain scanout result types shared by renderer implementations.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanoutPresentResult {
    pub surface_id: Option<u32>,
    pub readback_ok: Option<bool>,
    pub blit_duration_ns: u64,
    pub readback_duration_ns: u64,
}

/// Handle to a present the renderer is still executing.
///
/// Opaque to the device: only the backend that produced it knows how to poll
/// it. Holding one means the worker owns the readback buffer, so it must be
/// collected (blocking if necessary) before the resource or device goes away.
pub struct ScanoutPresentPending {
    receiver: std::sync::mpsc::Receiver<(ScanoutPresentResult, Option<Vec<u8>>)>,
}

impl ScanoutPresentPending {
    pub fn new(
        receiver: std::sync::mpsc::Receiver<(ScanoutPresentResult, Option<Vec<u8>>)>,
    ) -> Self {
        Self { receiver }
    }

    /// Collect the present if it has finished. `block` waits for it, which is
    /// required before teardown: the worker owns the readback buffer until it
    /// answers, so abandoning it would leave the renderer writing into memory
    /// the device is about to reuse.
    pub fn collect(&mut self, block: bool) -> Option<(ScanoutPresentResult, Option<Vec<u8>>)> {
        if block {
            // A disconnect means the renderer thread died; nothing to collect.
            self.receiver.recv().ok()
        } else {
            self.receiver.try_recv().ok()
        }
    }
}

impl std::fmt::Debug for ScanoutPresentPending {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ScanoutPresentPending")
    }
}
