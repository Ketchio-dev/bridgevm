//! VirtioPciBlock: BAR offset to MMIO register mapping, MSI-X BAR access, forwarding.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::msix::MsixTable;
use crate::pcie::VIRTIO_BLK_MSIX_PBA_OFFSET;
use crate::pcie::VIRTIO_BLK_MSIX_TABLE_OFFSET;
use crate::pcie::VIRTIO_BLK_MSIX_VECTOR_COUNT;
use std::io;
use std::path::Path;
#[path = "pci_register_map.rs"]
mod pci_register_map;
use pci_register_map::pci_to_mmio_offset;

#[derive(Debug)]
pub struct VirtioPciBlock {
    pub(crate) block: VirtioMmioBlock,
    pub(crate) msix: MsixTable,
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
            VirtioPciBlockOp::Read { size } if offset == PCI_ISR_CFG_OFFSET => {
                VirtioMmioBlockResult::ReadValue(mask_to_size(
                    u64::from(std::mem::take(&mut self.block.interrupt_status)),
                    size,
                ))
            }
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
