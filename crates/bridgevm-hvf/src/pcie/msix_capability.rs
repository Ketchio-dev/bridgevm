//! The MSI-X capability structure and its on-wire encoding.

use super::*;

/// A built MSI-X capability structure, ready to splice into an endpoint's
/// capability list. Future NVMe / virtio-pci devices register one of these so
/// the guest driver can program per-vector message addresses.
///
/// The on-wire layout (PCIe spec §7.7.2) is a 12-byte capability:
/// ```text
///   +0  Cap ID (0x11)   +1  Next-cap ptr
///   +2  Message Control (16-bit): bits 0..10 = table size - 1, bit 15 = enable
///   +4  Table   Offset/BIR (32-bit): bits 0..2 = BIR, bits 3.. = table offset
///   +8  PBA     Offset/BIR (32-bit): bits 0..2 = BIR, bits 3.. = PBA   offset
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MsixCapability {
    /// Number of interrupt vectors in the table (1..=2048).
    pub table_size: u16,
    /// BAR index (BIR) holding the MSI-X table.
    pub table_bir: u8,
    /// Byte offset of the table within `table_bir`'s BAR (must be 8-byte aligned).
    pub table_offset: u32,
    /// BAR index (BIR) holding the Pending Bit Array.
    pub pba_bir: u8,
    /// Byte offset of the PBA within `pba_bir`'s BAR (must be 8-byte aligned).
    pub pba_offset: u32,
}

impl MsixCapability {
    /// Total bytes of the MSI-X capability structure in config space.
    pub const SIZE_BYTES: u16 = 12;

    /// Bytes per MSI-X table entry (addr lo/hi, data, vector control).
    pub const ENTRY_BYTES: u32 = 16;

    /// Maximum encodable table size (the size field is 11 bits: `size - 1`).
    pub const MAX_TABLE_SIZE: u16 = 2048;

    /// Build a capability with `table_size` vectors whose table and PBA live in
    /// `bir` at `table_offset` / `pba_offset`. Panics on an out-of-range table
    /// size, an out-of-range BIR (0..=5), or a misaligned offset — the same
    /// fail-fast style as the rest of the platform model.
    pub fn new(table_size: u16, bir: u8, table_offset: u32, pba_offset: u32) -> Self {
        Self::with_birs(table_size, bir, table_offset, bir, pba_offset)
    }

    /// Build a capability whose table and PBA may live in different BARs.
    pub fn with_birs(
        table_size: u16,
        table_bir: u8,
        table_offset: u32,
        pba_bir: u8,
        pba_offset: u32,
    ) -> Self {
        assert!(
            (1..=Self::MAX_TABLE_SIZE).contains(&table_size),
            "MSI-X table size {table_size} out of range 1..=2048"
        );
        assert!((table_bir as usize) < NUM_BARS, "table BIR out of range");
        assert!((pba_bir as usize) < NUM_BARS, "PBA BIR out of range");
        assert!(
            table_offset % 8 == 0,
            "MSI-X table offset must be 8-byte aligned"
        );
        assert!(
            pba_offset % 8 == 0,
            "MSI-X PBA offset must be 8-byte aligned"
        );
        Self {
            table_size,
            table_bir,
            table_offset,
            pba_bir,
            pba_offset,
        }
    }

    /// The Message Control word: `table_size - 1` in bits 0..10. The MSI-X
    /// enable (bit 15) and function-mask (bit 14) bits start clear; the guest
    /// driver sets them.
    pub fn message_control(&self) -> u16 {
        (self.table_size - 1) & 0x07FF
    }

    /// The Table Offset/BIR dword: BIR in bits 0..2, offset (8-byte aligned) in
    /// the upper bits.
    pub fn table_offset_bir(&self) -> u32 {
        (self.table_offset & !0x7) | u32::from(self.table_bir & 0x7)
    }

    /// The PBA Offset/BIR dword.
    pub fn pba_offset_bir(&self) -> u32 {
        (self.pba_offset & !0x7) | u32::from(self.pba_bir & 0x7)
    }

    /// Total bytes the MSI-X table occupies in its BAR.
    pub fn table_byte_size(&self) -> u32 {
        u32::from(self.table_size) * Self::ENTRY_BYTES
    }

    /// Serialise the 12-byte capability with `next` as the next-cap pointer
    /// (`0` terminates the list). The caller splices this at the capability's
    /// config-space offset.
    pub fn to_bytes(&self, next: u8) -> [u8; Self::SIZE_BYTES as usize] {
        let mut bytes = [0u8; Self::SIZE_BYTES as usize];
        bytes[0] = CAP_ID_MSIX;
        bytes[1] = next;
        bytes[2..4].copy_from_slice(&self.message_control().to_le_bytes());
        bytes[4..8].copy_from_slice(&self.table_offset_bir().to_le_bytes());
        bytes[8..12].copy_from_slice(&self.pba_offset_bir().to_le_bytes());
        bytes
    }
}
