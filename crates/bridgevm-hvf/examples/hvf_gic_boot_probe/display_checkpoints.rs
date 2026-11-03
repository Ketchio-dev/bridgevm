//! Framebuffer checkpoints off the platform lock. A scheduled RAMFB sample
//! (`BRIDGEVM_RAMFB_SAMPLE_MS`, three during boot by default) and a BOOT_TIMER
//! display sample read the whole frame, checksum it and count its colours, and
//! a sample also writes raw and PPM files. That took up to 10 ms for a
//! 1280x800 frame inside vCPU0's automation block, under the platform lock, so
//! every other vCPU's next exit waited for it.
//!
//! Only their inputs come from the platform: which samples are due, the ramfb
//! config, the virtio-gpu scanout (copied) and the serial log the boot timer
//! scans. Those are taken under the lock. The frame is read from guest RAM,
//! summarized and written once the lock is released, as the ramfb display
//! exporter does (ramfb_display_thread.rs): other vCPUs keep drawing either
//! way, so a frame was as likely to be torn before. The lines print in the
//! order the locked pass printed them.

use crate::*;
use bridgevm_hvf::virtio_gpu::VirtioGpuScanout;

/// The platform state a pass's checkpoints read.
#[derive(Default)]
struct FrameSource {
    ramfb: Option<RamfbConfig>,
    gpu: Option<OwnedScanout>,
}

struct OwnedScanout {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    stride: u32,
    fourcc: u32,
}

impl FrameSource {
    fn take(platform: &VirtPlatform) -> Self {
        Self {
            ramfb: platform.ramfb_config(),
            gpu: platform.virtio_gpu_scanout().map(|scanout| OwnedScanout {
                bytes: scanout.bytes.to_vec(),
                width: scanout.width,
                height: scanout.height,
                stride: scanout.stride,
                fourcc: scanout.fourcc,
            }),
        }
    }

    fn gpu(&self) -> Option<VirtioGpuScanout<'_>> {
        self.gpu.as_ref().map(|scanout| VirtioGpuScanout {
            bytes: &scanout.bytes,
            width: scanout.width,
            height: scanout.height,
            stride: scanout.stride,
            fourcc: scanout.fourcc,
        })
    }
}

/// One automation pass's due framebuffer checkpoints.
pub(crate) struct DisplayCheckpoints {
    labels: Vec<String>,
    boot: BootTimerTick,
    frame: FrameSource,
}

impl DisplayCheckpoints {
    /// Under the platform lock: the samples now due, the boot timer's serial
    /// scan, and the frame source they read (only when one is due).
    pub(crate) fn take(
        platform: &VirtPlatform,
        samples: &mut RamfbSampleLoop,
        boot_timer: &mut BootTimer,
        vcpu: HvVcpuT,
        exit: u64,
        cpu0_pc: u64,
    ) -> Self {
        let mut labels = Vec::new();
        samples.emit_due(vcpu, |label| labels.push(label.to_owned()));
        let boot = boot_timer.take_tick(platform.uart_output(), exit, cpu0_pc);
        let frame = if labels.is_empty() && !boot.samples_display() {
            FrameSource::default()
        } else {
            FrameSource::take(platform)
        };
        Self {
            labels,
            boot,
            frame,
        }
    }

    /// Release `guard`, then `dump` each sample and print the boot timer's
    /// lines. Production dumps with `ramfb_dump::print_checkpoint_with_virtio_gpu`.
    pub(crate) fn emit_after<G>(
        self,
        guard: G,
        boot_timer: &mut BootTimer,
        mem: &dyn GuestMemoryMut,
        mut dump: impl FnMut(
            &str,
            Option<VirtioGpuScanout<'_>>,
            Option<RamfbConfig>,
            &dyn GuestMemoryMut,
        ),
    ) {
        drop(guard);
        for label in &self.labels {
            dump(label, self.frame.gpu(), self.frame.ramfb, mem);
        }
        boot_timer.finish_tick(self.boot, self.frame.gpu(), self.frame.ramfb, mem);
    }
}

#[cfg(test)]
#[path = "display_checkpoints_tests.rs"]
mod tests;
