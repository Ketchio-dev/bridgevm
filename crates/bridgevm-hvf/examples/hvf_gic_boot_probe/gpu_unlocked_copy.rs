//! virtio-gpu 2D frame copies off the platform lock (the library's
//! virtio_gpu/unlocked_copy.rs): the setting, and the kicking vCPU's run of
//! each copy between two holds.

use crate::platform_lock_holds::HeldPlatform;
use crate::*;
use bridgevm_hvf::virtio_gpu::GpuCopy;

/// On for the 2D device unless `BRIDGEVM_VIRTIO_GPU_UNLOCKED_COPY=0`.
/// Checkpoints keep the copies inline: they capture none in flight.
pub(crate) fn configure_gpu_unlocked_copy(platform: &mut VirtPlatform) {
    if platform.virtio_gpu_stats().is_none() {
        return;
    }
    let var = |name| std::env::var_os(name).is_some_and(|v| !v.is_empty());
    let checkpoint = var("BRIDGEVM_CHECKPOINT_STATE") || var("BRIDGEVM_RESTORE_STATE");
    let setting = std::env::var("BRIDGEVM_VIRTIO_GPU_UNLOCKED_COPY").ok();
    let wanted = gpu_unlocked_copy(setting.as_deref(), checkpoint);
    let on = platform.set_virtio_gpu_unlocked_copy(wanted);
    println!(
        "virtio-gpu 2D copies: {}",
        if on {
            "unlocked (the kicking vCPU copies without the platform lock; BRIDGEVM_VIRTIO_GPU_UNLOCKED_COPY=0 opts out)"
        } else if wanted {
            "under the platform lock (3D device)"
        } else if checkpoint {
            "under the platform lock (checkpoints keep copies inline)"
        } else {
            "under the platform lock (BRIDGEVM_VIRTIO_GPU_UNLOCKED_COPY=0)"
        }
    );
}

fn gpu_unlocked_copy(setting: Option<&str>, checkpoint: bool) -> bool {
    setting != Some("0") && !checkpoint
}

impl RunLoopDrainStats {
    /// Run the 2D copy a virtio-gpu kick queued, which the exit took under
    /// its first hold, with the platform unlocked. Then `lock` again: the
    /// finish posts the command's completion and runs the queues that waited,
    /// whose own copy, if they queued one, runs the same way; this hold's
    /// drain delivers the MSI-X.
    pub(crate) fn run_gpu_copies<'a>(
        &mut self,
        mut copy: Option<GpuCopy>,
        mut lock: impl FnMut() -> HeldPlatform<'a>,
        mem: &mut dyn GuestMemoryMut,
        trace: DrainTrace,
        context: DrainContext,
    ) -> Result<(), InterruptDeliveryError> {
        while let Some(mut job) = copy.take() {
            job.run(mem);
            let pending = {
                let mut platform = lock();
                copy = platform.finish_virtio_gpu_copy(job, mem);
                self.prepare_pending_delivery(&mut platform, mem, trace, context)
            };
            self.finish_prepared_interrupt_delivery(pending, trace)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::gpu_unlocked_copy;

    #[test]
    fn unlocked_copies_are_the_default_except_opted_out_or_checkpointing() {
        assert!(gpu_unlocked_copy(None, false));
        assert!(gpu_unlocked_copy(Some("1"), false));
        assert!(!gpu_unlocked_copy(Some("0"), false));
        assert!(!gpu_unlocked_copy(None, true));
    }
}
