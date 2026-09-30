//! Public full-frame writer for the BVFB shared-memory display file the app
//! maps (`BRIDGEVM_DISPLAY_EXPORT_FB`).
//!
//! The virtio-gpu device publishes its scanout through the same crate-private
//! sink. This wrapper lets a host-side exporter publish frames from another
//! source, such as the probe's ramfb exporter on a machine without virtio-gpu,
//! through that one header and seqlock implementation. A file must have one
//! writer: each writer keeps its own sequence counter, so two writers on one
//! file would publish sequence values the reader cannot order.

use std::path::{Path, PathBuf};

use crate::virtio_gpu::FbSink;

/// Owner of one BVFB display file: magic `0x42564642`, version 1, width,
/// height, stride and fourcc as little-endian u32 at bytes 0..24, a seqlock
/// u64 at byte 24 (odd while a frame is being written) and pixels from byte 64.
#[derive(Debug)]
pub struct DisplayFramebuffer {
    sink: FbSink,
}

impl DisplayFramebuffer {
    /// The writer for `BRIDGEVM_DISPLAY_EXPORT_FB`; `None` when unset or empty.
    /// Nothing is created until the first publish.
    pub fn from_env() -> Option<Self> {
        FbSink::from_env().map(|sink| Self { sink })
    }

    /// A writer for `path`. Nothing is created until the first publish.
    pub fn at_path(path: impl Into<PathBuf>) -> Self {
        Self {
            sink: FbSink::at_path(path.into()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.sink.path
    }

    /// Sequence value after the last completed publish: 0 before the first,
    /// then even, advancing by 2 per published frame.
    pub fn sequence(&self) -> u64 {
        self.sink.seq
    }

    /// Publish one complete frame. The first publish by a writer, exactly like
    /// the virtio-gpu sink's first write in a process, opens the path in place,
    /// grows it to fit the frame but never shrinks it, and continues its sequence;
    /// a later frame that needs more room does the same. `InvalidInput` means
    /// the geometry overflows or `bytes` is shorter than `height * stride`, and
    /// nothing was written.
    pub fn publish(
        &mut self,
        width: u32,
        height: u32,
        stride: u32,
        fourcc: u32,
        bytes: &[u8],
    ) -> std::io::Result<()> {
        if self
            .sink
            .write_inner(width, height, stride, fourcc, bytes, None)?
        {
            return Ok(());
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "display frame geometry overflows or is larger than its bytes",
        ))
    }
}

#[cfg(test)]
#[path = "display_fb_tests.rs"]
mod tests;
