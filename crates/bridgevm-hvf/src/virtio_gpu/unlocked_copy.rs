//! 2D frame copies without the platform lock. viogpudo kicks the control
//! queue once per command, and each kick is a data-abort exit that holds the
//! platform lock. A full TRANSFER_TO_HOST_2D (guest backing into the
//! resource) or RESOURCE_FLUSH (resource into the scanout, then into the
//! display file) costs about 1 ms at 3456x2234 (Retina slice 1, run 2), all of
//! it under that lock, so the other vCPUs and the HDA tick waited for it.
//!
//! With unlocked copies on, such a command is checked under the lock as
//! before, and its copy becomes a [`GpuCopy`] that owns what it touches: the
//! resource (taken out of the map) and, for a flush, the scanout buffer and
//! the display-file writer. The kicking vCPU takes it before it unlocks
//! ([`VirtioGpu::take_copy`]), runs it in its own view of guest RAM
//! ([`GpuCopy::run`]), locks again and finishes it ([`VirtioGpu::finish_copy`]):
//! the pieces go back, the chain's response and used-ring entry are written
//! and its interrupt is raised, so the guest sees the completion after the
//! copy, as inline. The control and cursor queues do not run while a copy is
//! out; their kicks meanwhile wait in the avail rings, and the finish runs both
//! queues, so commands keep their order and each completes once.
//!
//! A device reset while a copy runs clears the queues and resources as
//! always; its finish (from an older reset epoch) then posts nothing and only
//! gives the display-file writer back. While a flush is out, `scanout()`
//! reports the binding with no bytes. Only a vCPU that stops on a fatal error
//! between take and finish drops a copy, which leaves the queues stopped for
//! the rest of that (ending) run. Live VMMs enable this; tests, checkpoints
//! and the 3D device keep the inline copy.

use super::compositor_2d::composite_resource_to_scanout;
use super::scanout_publish::publish_fb_damage;
use super::*;
use crate::fwcfg::GuestMemoryMut;

/// One TRANSFER_TO_HOST_2D or RESOURCE_FLUSH whose pixels are copied after the
/// kicking vCPU drops the platform lock. Run it, then hand it back under the
/// lock to [`VirtioPciGpu::finish_copy`]; its queues stay stopped until then.
#[must_use = "the virtio-gpu queues stay stopped until the copy is finished"]
#[derive(Debug)]
pub struct GpuCopy {
    epoch: u64,
    resource_id: u32,
    resource: GpuResource,
    rect: Rect,
    work: CopyWork,
    ran: bool,
    chain: Option<CopyChain>,
}

#[derive(Debug)]
enum CopyWork {
    Transfer {
        offset: u64,
    },
    Flush {
        scanout: Vec<u8>,
        width: u32,
        height: u32,
        fb_sink: Option<FbSink>,
    },
}

/// The control-queue chain the copy completes.
#[derive(Debug)]
struct CopyChain {
    queue: VirtioGpuQueue,
    head: u16,
    descs: Vec<Descriptor>,
    response: Vec<u8>,
}

/// Whether a copy holds the device's pieces.
#[derive(Debug, Default)]
pub(crate) enum CopyState {
    #[default]
    Idle,
    /// Queued by the command just handled, for the vCPU to take.
    Queued(Box<GpuCopy>),
    /// Taken: the vCPU is copying.
    Running,
}

impl GpuCopy {
    /// Copy the pixels. `mem` is the calling vCPU's view of guest RAM.
    pub fn run(&mut self, mem: &dyn GuestMemoryMut) {
        match &mut self.work {
            CopyWork::Transfer { offset } => {
                copy_backing_to_resource(mem, &mut self.resource, self.rect, *offset)
            }
            CopyWork::Flush {
                scanout,
                width,
                height,
                fb_sink,
            } => {
                composite_resource_to_scanout(scanout, *width, *height, &self.resource, self.rect);
                publish_fb_damage(fb_sink.as_mut(), scanout, *width, *height, self.rect);
            }
        }
        self.ran = true;
    }
}

impl VirtioGpu {
    /// Whether a copy holds the device's pieces, so neither queue may run.
    pub(crate) fn copy_pending(&self) -> bool {
        !matches!(self.copy, CopyState::Idle)
    }

    /// Queue the copy of the resource just checked, instead of copying under
    /// the lock. `false` (unlocked copies off): copy inline.
    fn queue_copy(
        &mut self,
        resource_id: u32,
        rect: Rect,
        work: impl FnOnce(&mut Self) -> CopyWork,
    ) -> bool {
        if !self.unlocked_copy {
            return false;
        }
        // Commands run only while no copy is out (process_queue).
        debug_assert!(!self.copy_pending());
        let Some(resource) = self.resources.remove(&resource_id) else {
            return false;
        };
        let work = work(self);
        self.copy = CopyState::Queued(Box::new(GpuCopy {
            epoch: self.copy_epoch,
            resource_id,
            resource,
            rect,
            work,
            ran: false,
            chain: None,
        }));
        true
    }

    pub(crate) fn queue_transfer(&mut self, resource_id: u32, rect: Rect, offset: u64) -> bool {
        self.queue_copy(resource_id, rect, |_| CopyWork::Transfer { offset })
    }

    pub(crate) fn queue_flush(&mut self, resource_id: u32, rect: Rect) -> bool {
        self.queue_copy(resource_id, rect, |gpu| CopyWork::Flush {
            scanout: std::mem::take(&mut gpu.scanout),
            width: gpu.width,
            height: gpu.height,
            fb_sink: gpu.fb_sink.take(),
        })
    }

    /// The chain whose command queued a copy completes with it. `false`: the
    /// command queued none, so the chain completes now.
    pub(crate) fn defer_chain_to_copy(
        &mut self,
        queue: &VirtioGpuQueue,
        head: u16,
        descs: &mut Vec<Descriptor>,
        response: &mut Vec<u8>,
    ) -> bool {
        let CopyState::Queued(copy) = &mut self.copy else {
            return false;
        };
        debug_assert!(copy.chain.is_none());
        copy.chain = Some(CopyChain {
            queue: *queue,
            head,
            descs: std::mem::take(descs),
            response: std::mem::take(response),
        });
        true
    }

    /// The copy the last kick queued, for the vCPU to run once it unlocks.
    pub(crate) fn take_copy(&mut self) -> Option<GpuCopy> {
        match std::mem::replace(&mut self.copy, CopyState::Running) {
            CopyState::Queued(copy) => Some(*copy),
            other => {
                self.copy = other;
                None
            }
        }
    }

    /// Under the lock again: put the copy's pieces back, complete its chain
    /// and run the queues, whose kicks waited for it. A copy from before a
    /// device reset only gives the display-file writer back. A copy not run
    /// yet runs here, under the lock.
    pub(crate) fn finish_copy(&mut self, mut copy: GpuCopy, mem: &mut dyn GuestMemoryMut) {
        debug_assert!(matches!(self.copy, CopyState::Running));
        self.copy = CopyState::Idle;
        if copy.epoch != self.copy_epoch {
            if let CopyWork::Flush { fb_sink, .. } = copy.work {
                self.fb_sink = fb_sink;
            }
        } else {
            if !copy.ran {
                copy.run(mem);
            }
            self.resources.insert(copy.resource_id, copy.resource);
            if let CopyWork::Flush {
                scanout, fb_sink, ..
            } = copy.work
            {
                self.scanout = scanout;
                self.fb_sink = fb_sink;
            }
            if let Some(chain) = copy.chain {
                let used_len = Self::scatter_write(mem, &chain.descs, &chain.response);
                Self::write_used(mem, &chain.queue, chain.head, used_len);
                self.mark_queue_interrupt(QUEUE_CONTROL);
                self.recycle_parked_response_buffers(chain.descs, chain.response);
            }
        }
        self.process_control_queue(mem);
        self.process_cursor_queue(mem);
    }
}

impl VirtioPciGpu {
    /// Live VMMs enable this; a device with a 3D backend keeps its copies
    /// inline, since its readbacks share the scanout. Returns whether it is on.
    pub fn set_unlocked_copy(&mut self, enabled: bool) -> bool {
        self.gpu.unlocked_copy = enabled && !self.gpu.three_d.has_backend();
        self.gpu.unlocked_copy
    }

    /// See [`VirtioGpu::take_copy`].
    pub fn take_copy(&mut self) -> Option<GpuCopy> {
        self.gpu.take_copy()
    }

    /// See [`VirtioGpu::finish_copy`]; the caller flushes the MSI-X.
    pub fn finish_copy(&mut self, copy: GpuCopy, mem: &mut dyn GuestMemoryMut) {
        self.gpu.finish_copy(copy, mem);
        self.gpu.drain_completed_fences(mem);
    }
}
