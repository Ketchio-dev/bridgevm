//! Interrupt delivery: ISR bit, per-queue MSI-X bookkeeping, table/PBA access, vector raising.

use super::*;
use crate::msix::MsixMessage;
use crate::pcie::VIRTIO_GPU_MSIX_PBA_OFFSET;
use crate::pcie::VIRTIO_GPU_MSIX_TABLE_OFFSET;

pub(crate) fn queue_bit(index: usize) -> Option<u8> {
    (index < u8::BITS as usize).then(|| 1u8 << index)
}

impl VirtioGpu {
    pub fn interrupt_line_level(&self) -> bool {
        self.interrupt_status != 0
    }

    pub(crate) fn mark_queue_interrupt(&mut self, queue_index: usize) {
        if let Some(queue) = self.queues.get_mut(queue_index) {
            queue.pending_msix = true;
            if let Some(bit) = queue_bit(queue_index) {
                self.pending_msix_queue_bits |= bit;
            }
        }
        self.interrupt_status |= 1;
    }
}

impl VirtioPciGpu {
    pub fn interrupt_line_level(&self) -> bool {
        self.gpu.interrupt_line_level()
    }

    pub fn msix_bar_access(&mut self, offset: u64, op: VirtioPciGpuOp) -> VirtioGpuResult {
        if let Some(table_offset) = self.msix_table_offset(offset) {
            return match op {
                VirtioPciGpuOp::Read { size } => {
                    VirtioGpuResult::ReadValue(self.msix.table_read(table_offset, size))
                }
                VirtioPciGpuOp::Write { size, value } => {
                    self.msix.table_write(table_offset, size, value);
                    VirtioGpuResult::WriteAck
                }
            };
        }
        if let Some(pba_offset) = self.msix_pba_offset(offset) {
            return match op {
                VirtioPciGpuOp::Read { size } => {
                    VirtioGpuResult::ReadValue(self.msix.pba_read(pba_offset, size))
                }
                VirtioPciGpuOp::Write { size, value } => {
                    self.msix.pba_write(pba_offset, size, value);
                    VirtioGpuResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciGpuOp::Read { .. } => VirtioGpuResult::ReadValue(0),
            VirtioPciGpuOp::Write { .. } => VirtioGpuResult::WriteAck,
        }
    }

    pub fn raise_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        self.raise_pending_msix_into(function_enabled, function_masked, &mut messages);
        messages
    }

    pub fn raise_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        if self.gpu.pending_config_change {
            let vector = self.gpu.config_msix_vector;
            if vector != VIRTIO_MSI_NO_VECTOR {
                if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                    self.gpu.pending_config_change = false;
                    venus_start_trace_msix(
                        "config raised",
                        vector,
                        function_enabled,
                        function_masked,
                    );
                    out.push(message);
                } else {
                    venus_start_trace_msix(
                        "config held",
                        vector,
                        function_enabled,
                        function_masked,
                    );
                }
            } else {
                // No config vector programmed (INTx path): the ISR config bit
                // is already set; nothing MSI-X to raise.
                self.gpu.pending_config_change = false;
            }
        }
        let mut pending = self.gpu.pending_msix_queue_bits;
        while pending != 0 {
            let queue_index = pending.trailing_zeros() as usize;
            let vector = self.gpu.queues[queue_index].msix_vector;
            if vector == VIRTIO_MSI_NO_VECTOR {
                venus_start_trace_msix_queue("no-vector (ISR path)", queue_index, vector);
                pending &= !(1u8 << queue_index);
                continue;
            }
            if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                self.gpu.queues[queue_index].pending_msix = false;
                self.gpu.pending_msix_queue_bits &= !(1u8 << queue_index);
                venus_start_trace_msix_queue("raised", queue_index, vector);
                out.push(message);
            } else {
                venus_start_trace_msix_queue("held (disabled/masked)", queue_index, vector);
            }
            pending &= !(1u8 << queue_index);
        }
    }

    pub fn drain_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        self.drain_pending_msix_into(function_enabled, function_masked, &mut messages);
        messages
    }

    pub fn drain_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        let start = out.len();
        self.msix
            .drain_pending_into(function_enabled, function_masked, out);
        for message in &out[start..] {
            self.clear_pending_queue_for_vector(message.vector);
        }
        self.raise_pending_msix_into(function_enabled, function_masked, out);
    }

    pub(crate) fn clear_pending_queue_for_vector(&mut self, vector: u16) {
        for (queue_index, queue) in self.gpu.queues.iter_mut().enumerate() {
            if queue.msix_vector == vector {
                queue.pending_msix = false;
                if let Some(bit) = queue_bit(queue_index) {
                    self.gpu.pending_msix_queue_bits &= !bit;
                }
            }
        }
    }

    pub(crate) fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_GPU_MSIX_TABLE_OFFSET))?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    pub(crate) fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_GPU_MSIX_PBA_OFFSET))?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
    }
}
