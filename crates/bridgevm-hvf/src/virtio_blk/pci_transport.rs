//! VirtioPciBlock: BAR offset to MMIO register mapping, MSI-X BAR access, forwarding.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::msix::MsixTable;
use crate::pcie::VIRTIO_BLK_MSIX_PBA_OFFSET;
use crate::pcie::VIRTIO_BLK_MSIX_TABLE_OFFSET;
use crate::pcie::VIRTIO_BLK_MSIX_VECTOR_COUNT;
use std::io;
use std::path::Path;

#[derive(Debug)]
pub struct VirtioPciBlock {
    pub(crate) block: VirtioMmioBlock,
    pub(crate) msix: MsixTable,
}

pub(crate) fn pci_to_mmio_offset(offset: u64, is_write: bool) -> Option<u64> {
    if (PCI_COMMON_CFG_OFFSET..PCI_COMMON_CFG_OFFSET + PCI_CFG_REGION_SIZE).contains(&offset) {
        return Some(offset - PCI_COMMON_CFG_OFFSET);
    }
    if (PCI_DEVICE_CFG_OFFSET..PCI_DEVICE_CFG_OFFSET + PCI_CFG_REGION_SIZE).contains(&offset) {
        return Some(REG_CONFIG + offset - PCI_DEVICE_CFG_OFFSET);
    }
    if (PCI_NOTIFY_CFG_OFFSET..PCI_NOTIFY_CFG_OFFSET + PCI_CFG_REGION_SIZE).contains(&offset) {
        return Some(REG_QUEUE_NOTIFY);
    }
    if offset == PCI_ISR_CFG_OFFSET {
        return Some(if is_write {
            REG_INTERRUPT_ACK
        } else {
            REG_INTERRUPT_STATUS
        });
    }
    None
}

impl VirtioPciBlock {
    pub fn open_read_only(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            block: VirtioMmioBlock::open_read_only_modern(path)?,
            msix: MsixTable::new(VIRTIO_BLK_MSIX_VECTOR_COUNT),
        })
    }

    pub fn stats(&self) -> VirtioMmioBlockStats {
        self.block.stats()
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.block.interrupt_line_level()
    }

    pub fn recent_request_trace(&self) -> Vec<VirtioBlockRequestTrace> {
        self.block.recent_request_trace()
    }

    pub fn reset_runtime_state(&mut self) {
        self.block.reset_runtime_state();
        self.msix = MsixTable::new(VIRTIO_BLK_MSIX_VECTOR_COUNT);
    }

    pub fn access(
        &mut self,
        offset: u64,
        op: VirtioPciBlockOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioMmioBlockResult {
        let is_write = matches!(op, VirtioPciBlockOp::Write { .. });
        let Some(mmio_offset) = pci_to_mmio_offset(offset, is_write) else {
            return match op {
                VirtioPciBlockOp::Read { .. } => VirtioMmioBlockResult::ReadValue(0),
                VirtioPciBlockOp::Write { .. } => VirtioMmioBlockResult::WriteAck,
            };
        };
        match op {
            VirtioPciBlockOp::Read { size } => self.block.access(mmio_offset, false, size, 0, mem),
            VirtioPciBlockOp::Write { size, value } => {
                self.block.access(mmio_offset, true, size, value, mem)
            }
        }
    }

    pub fn legacy_io_access(
        &mut self,
        offset: u64,
        op: VirtioPciBlockOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioMmioBlockResult {
        match op {
            VirtioPciBlockOp::Read { size } => {
                VirtioMmioBlockResult::ReadValue(self.block.legacy_pci_io_read(offset, size))
            }
            VirtioPciBlockOp::Write { size, value } => {
                self.block.legacy_pci_io_write(offset, size, value, mem);
                VirtioMmioBlockResult::WriteAck
            }
        }
    }

    pub fn msix_bar_access(&mut self, offset: u64, op: VirtioPciBlockOp) -> VirtioMmioBlockResult {
        if let Some(table_offset) = self.msix_table_offset(offset) {
            return match op {
                VirtioPciBlockOp::Read { size } => {
                    VirtioMmioBlockResult::ReadValue(self.msix.table_read(table_offset, size))
                }
                VirtioPciBlockOp::Write { size, value } => {
                    self.msix.table_write(table_offset, size, value);
                    VirtioMmioBlockResult::WriteAck
                }
            };
        }
        if let Some(pba_offset) = self.msix_pba_offset(offset) {
            return match op {
                VirtioPciBlockOp::Read { size } => {
                    VirtioMmioBlockResult::ReadValue(self.msix.pba_read(pba_offset, size))
                }
                VirtioPciBlockOp::Write { size, value } => {
                    self.msix.pba_write(pba_offset, size, value);
                    VirtioMmioBlockResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciBlockOp::Read { .. } => VirtioMmioBlockResult::ReadValue(0),
            VirtioPciBlockOp::Write { .. } => VirtioMmioBlockResult::WriteAck,
        }
    }

    pub(crate) fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_BLK_MSIX_TABLE_OFFSET))?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    pub(crate) fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_BLK_MSIX_PBA_OFFSET))?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
    }
}
