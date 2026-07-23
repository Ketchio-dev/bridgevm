//! Host vblank pacing and the vsync NOPs parked on it.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use std::fmt::Write as _;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

/// Lock-free wake signal for host vblank pacing. The device (behind the
/// platform mutex, on a vCPU thread) publishes "a parked vsync NOP exists and
/// becomes due at `deadline_ns`"; a host waker thread reads it WITHOUT any
/// lock and forces a vCPU exit only when the deadline has passed, so the
/// per-exit drain retires the NOP even while the guest idles in WFI. This is
/// the piece the earlier host-pacing attempts lacked: a host thread must never
/// contend for the platform mutex (vCPU threads hold it almost continuously
/// under 3D load), and it must never force exits unconditionally (exit storm).
#[derive(Debug)]
pub struct VblankWakeState {
    pub(crate) base: Instant,
    pub(crate) parked: AtomicBool,
    pub(crate) deadline_ns: AtomicU64,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingVblankResponse {
    pub(crate) queue_index: usize,
    pub(crate) queue: VirtioGpuQueue,
    pub(crate) head: u16,
    pub(crate) descs: Vec<Descriptor>,
    pub(crate) response: Vec<u8>,
}

impl VblankWakeState {
    pub fn new() -> Self {
        Self {
            base: Instant::now(),
            parked: AtomicBool::new(false),
            deadline_ns: AtomicU64::new(0),
        }
    }

    pub(crate) fn publish(&self, parked: bool, deadline: Option<Instant>) {
        let deadline_ns = deadline
            .map(|d| {
                u64::try_from(d.saturating_duration_since(self.base).as_nanos()).unwrap_or(u64::MAX)
            })
            .unwrap_or(0);
        self.deadline_ns.store(deadline_ns, Ordering::SeqCst);
        self.parked.store(parked, Ordering::SeqCst);
    }

    pub fn parked(&self) -> bool {
        self.parked.load(Ordering::SeqCst)
    }

    /// Time remaining until the parked NOP is due, `Duration::ZERO` when due
    /// now, or `None` when nothing is parked.
    pub fn time_to_deadline(&self, now: Instant) -> Option<Duration> {
        if !self.parked() {
            return None;
        }
        let deadline_ns = self.deadline_ns.load(Ordering::SeqCst);
        let now_ns =
            u64::try_from(now.saturating_duration_since(self.base).as_nanos()).unwrap_or(u64::MAX);
        Some(Duration::from_nanos(deadline_ns.saturating_sub(now_ns)))
    }
}

impl Default for VblankWakeState {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtioGpu {
    pub fn set_vblank_interval(&mut self, interval: Duration) {
        self.vblank_interval = interval;
        self.last_vblank = None;
        self.publish_vblank_wake();
        let enabled = !interval.is_zero();
        let interval_ns = interval.as_nanos();
        self.record_trace_fields("vblank_pacing_config", |fields| {
            let _ = write!(
                fields,
                ",\"enabled\":{enabled},\"interval_ns\":{interval_ns}"
            );
        });
    }

    /// Share the lock-free wake signal a host waker thread polls to bound
    /// vblank retire latency while the guest idles (no vCPU exits).
    pub fn set_vblank_wake(&mut self, wake: Arc<VblankWakeState>) {
        self.vblank_wake = Some(wake);
        self.publish_vblank_wake();
    }

    pub fn vblank_wake(&self) -> Option<Arc<VblankWakeState>> {
        self.vblank_wake.clone()
    }

    pub(crate) fn publish_vblank_wake(&self) {
        let Some(wake) = self.vblank_wake.as_ref() else {
            return;
        };
        let parked = !self.vblank_interval.is_zero() && !self.pending_vblank.is_empty();
        let deadline = self.last_vblank.map(|last| last + self.vblank_interval);
        wake.publish(parked, deadline);
    }

    pub fn drain_host_vblank(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.drain_host_vblank_at(mem, Instant::now());
    }

    pub(crate) fn drain_host_vblank_at(&mut self, mem: &mut dyn GuestMemoryMut, now: Instant) {
        if self.vblank_interval.is_zero() || self.pending_vblank.is_empty() {
            return;
        }
        if self
            .last_vblank
            .is_some_and(|last| now.saturating_duration_since(last) < self.vblank_interval)
        {
            return;
        }

        // Retire exactly one response. Even if the vCPU did not exit for several
        // intervals, do not catch up in a burst.
        let pending_response = self.pending_vblank.remove(0);
        let used_len =
            Self::scatter_write(mem, &pending_response.descs, &pending_response.response);
        Self::write_used(
            mem,
            &pending_response.queue,
            pending_response.head,
            used_len,
        );
        self.mark_queue_interrupt(pending_response.queue_index);
        // Anchor the next deadline on the absolute schedule, not the (late)
        // retire time, so wake/drain latency does not accumulate into a lower
        // long-run rate. Re-anchor at `now` only after a gap of more than one
        // interval (guest asleep) — never catch up in a burst.
        self.last_vblank = Some(match self.last_vblank {
            Some(last)
                if now.saturating_duration_since(last + self.vblank_interval)
                    <= self.vblank_interval =>
            {
                last + self.vblank_interval
            }
            _ => now,
        });
        self.vblank_paced_count = self.vblank_paced_count.saturating_add(1);
        self.publish_vblank_wake();

        let count = self.vblank_paced_count;
        let interval_ns = self.vblank_interval.as_nanos();
        let pending = self.pending_vblank.len();
        self.record_trace_fields("vblank_paced", |fields| {
            let _ = write!(
                fields,
                ",\"vblank_paced_count\":{count},\"interval_ns\":{interval_ns},\"used_len\":{used_len},\"pending\":{pending}"
            );
        });
        self.recycle_parked_response_buffers(pending_response.descs, pending_response.response);
    }
}
