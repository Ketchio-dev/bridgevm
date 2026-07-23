//! Type-0 config-space header offsets, command/status bits, sub-dword access helpers.

// ---- Type-0 config-space register offsets -----------------------------------

/// `0x00` Vendor ID (16-bit) + `0x02` Device ID (16-bit).
pub const REG_VENDOR_DEVICE: u16 = 0x00;

/// `0x04` Command (16-bit) + `0x06` Status (16-bit).
pub const REG_COMMAND_STATUS: u16 = 0x04;

/// `0x08` Revision ID (8-bit) + Class Code (24-bit).
pub const REG_REVISION_CLASS: u16 = 0x08;

/// `0x0c` Cache Line Size / Latency / Header Type / BIST.
pub const REG_BIST_HEADER: u16 = 0x0c;

/// First Base Address Register (`0x10`). A type-0 header has BAR0..BAR5.
pub const REG_BAR0: u16 = 0x10;

/// Capabilities pointer (8-bit at `0x34`).
pub const REG_CAP_PTR: u16 = 0x34;

pub const REG_SUBSYSTEM_IDS: u16 = 0x2c;

/// Interrupt Line (byte 0) and Interrupt Pin (byte 1).
pub const REG_INTERRUPT_LINE_PIN: u16 = 0x3c;

/// Number of Base Address Registers in a type-0 (endpoint) header.
pub const NUM_BARS: usize = 6;

/// Header Type byte: type-0 (endpoint), single-function.
pub const HEADER_TYPE_ENDPOINT: u8 = 0x00;

// Command-register bits the host bridge actually honours.
/// Command bit 0: respond to I/O-space accesses.
pub const CMD_IO_SPACE: u16 = 1 << 0;

/// Command bit 1: respond to memory-space accesses.
pub const CMD_MEMORY_SPACE: u16 = 1 << 1;

/// Command bit 2: act as a bus master (issue DMA).
pub const CMD_BUS_MASTER: u16 = 1 << 2;

/// Mask of command bits this model keeps writable; others read back as zero.
pub const CMD_WRITABLE_MASK: u16 = CMD_IO_SPACE | CMD_MEMORY_SPACE | CMD_BUS_MASTER;

/// Status register: capabilities-list present (bit 4). The host bridge has no
/// capability list, so this stays clear; endpoints that add MSI-X set it.
pub const STATUS_CAP_LIST: u16 = 1 << 4;

/// The value an ECAM read returns when no device answers: all-ones. Firmware
/// treats a `0xFFFF_FFFF` vendor/device read as "slot empty".
pub const NO_DEVICE: u64 = 0xFFFF_FFFF;

// ---- The ECAM device --------------------------------------------------------

// ---- sub-dword access helpers -----------------------------------------------

/// All-ones for an access of `size` bytes (1, 2, 4 -> 0xFF, 0xFFFF, 0xFFFFFFFF;
/// any other width clamps to a 32-bit all-ones, matching a full-dword read).
pub(crate) fn all_ones(size: u8) -> u64 {
    match size {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => 0xFFFF_FFFF,
        _ => 0xFFFF_FFFF,
    }
}

/// Extract the `size`-byte field at byte offset `reg` from a 32-bit dword
/// (little-endian config space).
pub(crate) fn extract(dword: u32, reg: u16, size: u8) -> u64 {
    let byte = (reg & 0x3) as u32;
    let shift = byte * 8;
    let value = (dword >> shift) as u64;
    match size {
        1 => value & 0xFF,
        2 => value & 0xFFFF,
        4 => value & 0xFFFF_FFFF,
        _ => value & 0xFFFF_FFFF,
    }
}

/// Merge a `size`-byte `value` written at byte offset `reg` into an existing
/// `dword` (read-modify-write for sub-dword config writes).
pub(crate) fn insert(dword: u32, reg: u16, size: u8, value: u64) -> u32 {
    let byte = (reg & 0x3) as u32;
    let shift = byte * 8;
    let width_mask: u32 = match size {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => 0xFFFF_FFFF,
        _ => 0xFFFF_FFFF,
    };
    let field_mask = width_mask.checked_shl(shift).unwrap_or(0);
    let placed = ((value as u32) & width_mask)
        .checked_shl(shift)
        .unwrap_or(0);
    (dword & !field_mask) | placed
}
