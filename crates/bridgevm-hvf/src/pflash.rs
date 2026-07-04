//! Minimal Intel P30-style NOR flash model for QEMU `virt` pflash variables.
//!
//! ArmVirtQemu's `VirtNorFlashDxe` does not treat the writable pflash bank as
//! plain RAM. It sends Intel StrataFlash command sequences and polls the status
//! register while updating the UEFI variable store. The Path A live probe used to
//! map the vars bank as writable RAM, so those command writes modified the backing
//! bytes and the firmware spun forever waiting for `P30_SR_BIT_WRITE`.

use crate::platform_virt::{MmioOp, MmioOutcome};

const P30_SR_WRITE_READY_8: u8 = 0x80;
const P30_CMD_READ_DEVICE_ID: u8 = 0x90;
const P30_CMD_READ_STATUS_REGISTER: u8 = 0x70;
const P30_CMD_CLEAR_STATUS_REGISTER: u8 = 0x50;
const P30_CMD_READ_ARRAY: u8 = 0xff;
const P30_CMD_READ_CFI_QUERY: u8 = 0x98;
const P30_CMD_WORD_PROGRAM_SETUP: u8 = 0x40;
const P30_CMD_ALT_WORD_PROGRAM_SETUP: u8 = 0x10;
const P30_CMD_BUFFERED_PROGRAM_SETUP: u8 = 0xe8;
const P30_CMD_BLOCK_ERASE_SETUP: u8 = 0x20;
const P30_CMD_LOCK_BLOCK_SETUP: u8 = 0x60;
const P30_CMD_CONFIRM: u8 = 0xd0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    ReadArray,
    ReadStatus,
    ReadId,
    ReadCfi,
    ProgramWord,
    EraseSetup { block_base: usize },
    LockSetup,
    AwaitBufferCount,
    BufferData { remaining_words: usize },
}

/// Writable pflash bank with just enough Intel P30 semantics for EDK2 variable
/// writes: status polling, block lock reads, erase, word program and buffered
/// program.
#[derive(Debug, Clone)]
pub struct P30NorFlash {
    base: u64,
    block_size: usize,
    bytes: Vec<u8>,
    mode: Mode,
}

impl P30NorFlash {
    pub fn new(base: u64, size: usize, block_size: usize) -> Self {
        assert!(block_size != 0, "pflash block size must be non-zero");
        Self {
            base,
            block_size,
            bytes: vec![0xff; size],
            mode: Mode::ReadArray,
        }
    }

    pub fn load(&mut self, data: &[u8]) {
        assert!(
            data.len() <= self.bytes.len(),
            "pflash image larger than bank"
        );
        self.bytes.fill(0xff);
        self.bytes[..data.len()].copy_from_slice(data);
        self.mode = Mode::ReadArray;
    }

    /// Snapshot of the whole pflash bank, including guest variable-store writes.
    pub fn image(&self) -> &[u8] {
        &self.bytes
    }

    pub fn reset_runtime_state(&mut self) {
        self.mode = Mode::ReadArray;
    }

    pub fn access(&mut self, gpa: u64, op: MmioOp) -> MmioOutcome {
        let Some(offset) = self.offset(gpa) else {
            return MmioOutcome::Unmapped;
        };
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.read(offset, size)),
            MmioOp::Write { size, value } => {
                self.write(offset, size, value);
                MmioOutcome::WriteAck
            }
        }
    }

    fn offset(&self, gpa: u64) -> Option<usize> {
        let offset = gpa.checked_sub(self.base)? as usize;
        (offset < self.bytes.len()).then_some(offset)
    }

    fn read(&self, offset: usize, size: u8) -> u64 {
        match self.mode {
            Mode::ReadStatus | Mode::AwaitBufferCount | Mode::BufferData { .. } => {
                status_value(size)
            }
            Mode::ReadId => 0,
            Mode::ReadCfi => cfi_value(offset, size),
            _ => self.read_array(offset, size),
        }
    }

    fn write(&mut self, offset: usize, size: u8, value: u64) {
        match self.mode {
            Mode::ProgramWord => {
                self.program(offset, size, value);
                self.mode = Mode::ReadStatus;
                return;
            }
            Mode::AwaitBufferCount => {
                let words = ((value & 0xff) as usize) + 1;
                self.mode = Mode::BufferData {
                    remaining_words: words,
                };
                return;
            }
            Mode::BufferData { remaining_words } if remaining_words > 0 => {
                self.program(offset, size, value);
                self.mode = Mode::BufferData {
                    remaining_words: remaining_words.saturating_sub(1),
                };
                return;
            }
            _ => {}
        }

        if let Some(cmd) = command_byte(value) {
            self.command(offset, cmd);
        }
    }

    fn command(&mut self, offset: usize, cmd: u8) {
        match cmd {
            P30_CMD_READ_ARRAY => self.mode = Mode::ReadArray,
            P30_CMD_READ_STATUS_REGISTER | P30_CMD_CLEAR_STATUS_REGISTER => {
                self.mode = Mode::ReadStatus
            }
            P30_CMD_READ_DEVICE_ID => self.mode = Mode::ReadId,
            P30_CMD_READ_CFI_QUERY => self.mode = Mode::ReadCfi,
            P30_CMD_WORD_PROGRAM_SETUP | P30_CMD_ALT_WORD_PROGRAM_SETUP => {
                self.mode = Mode::ProgramWord
            }
            P30_CMD_BUFFERED_PROGRAM_SETUP => self.mode = Mode::AwaitBufferCount,
            P30_CMD_BLOCK_ERASE_SETUP => {
                self.mode = Mode::EraseSetup {
                    block_base: self.block_base(offset),
                }
            }
            P30_CMD_LOCK_BLOCK_SETUP => self.mode = Mode::LockSetup,
            P30_CMD_CONFIRM => {
                if let Mode::EraseSetup { block_base } = self.mode {
                    self.erase_block(block_base);
                }
                self.mode = Mode::ReadStatus;
            }
            _ => {}
        }
    }

    fn read_array(&self, offset: usize, size: u8) -> u64 {
        let mut out = 0u64;
        for i in 0..usize::from(size) {
            let byte = self.bytes.get(offset + i).copied().unwrap_or(0xff);
            out |= u64::from(byte) << (i * 8);
        }
        out
    }

    fn program(&mut self, offset: usize, size: u8, value: u64) {
        for i in 0..usize::from(size) {
            if let Some(byte) = self.bytes.get_mut(offset + i) {
                *byte &= ((value >> (i * 8)) & 0xff) as u8;
            }
        }
    }

    fn erase_block(&mut self, block_base: usize) {
        let end = (block_base + self.block_size).min(self.bytes.len());
        self.bytes[block_base..end].fill(0xff);
    }

    fn block_base(&self, offset: usize) -> usize {
        offset - (offset % self.block_size)
    }
}

fn command_byte(value: u64) -> Option<u8> {
    let lo = (value & 0xff) as u8;
    let known = matches!(
        lo,
        P30_CMD_READ_DEVICE_ID
            | P30_CMD_READ_STATUS_REGISTER
            | P30_CMD_CLEAR_STATUS_REGISTER
            | P30_CMD_READ_ARRAY
            | P30_CMD_READ_CFI_QUERY
            | P30_CMD_WORD_PROGRAM_SETUP
            | P30_CMD_ALT_WORD_PROGRAM_SETUP
            | P30_CMD_BUFFERED_PROGRAM_SETUP
            | P30_CMD_CONFIRM
            | P30_CMD_BLOCK_ERASE_SETUP
            | P30_CMD_LOCK_BLOCK_SETUP
    );
    known.then_some(lo)
}

fn status_value(size: u8) -> u64 {
    match size {
        1 => u64::from(P30_SR_WRITE_READY_8),
        2 => u64::from(P30_SR_WRITE_READY_8),
        4 => 0x0080_0080,
        8 => 0x0080_0080_0080_0080,
        _ => 0,
    }
}

fn cfi_value(offset: usize, size: u8) -> u64 {
    // `VirtNorFlashDxe` asks for CFI "QRY" through 32-bit dual-chip accesses at
    // CREATE_NOR_ADDRESS(base, 0x10). That offset is 0x40 bytes from the bank base.
    if offset == 0x40 && size == 4 {
        0x0059_5251
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u64 = 0x0400_0000;

    fn flash() -> P30NorFlash {
        P30NorFlash::new(BASE, 0x80000, 0x40000)
    }

    fn read32(flash: &mut P30NorFlash, off: u64) -> u64 {
        match flash.access(BASE + off, MmioOp::Read { size: 4 }) {
            MmioOutcome::ReadValue(v) => v,
            other => panic!("unexpected read outcome: {other:?}"),
        }
    }

    fn write32(flash: &mut P30NorFlash, off: u64, value: u64) {
        assert_eq!(
            flash.access(BASE + off, MmioOp::Write { size: 4, value },),
            MmioOutcome::WriteAck
        );
    }

    #[test]
    fn reads_loaded_array_bytes_by_default() {
        let mut f = flash();
        f.load(&[0x78, 0x56, 0x34, 0x12]);
        assert_eq!(read32(&mut f, 0), 0x1234_5678);
        assert_eq!(f.image()[4], 0xff);
    }

    #[test]
    fn image_snapshot_reflects_program_and_erase() {
        let mut f = flash();
        f.load(&[0; 16]);
        assert_eq!(f.image()[0], 0);

        write32(&mut f, 0x100, 0x0040_0040);
        write32(&mut f, 0x100, 0x1234_5678);
        write32(&mut f, 0, 0x00ff_00ff);
        assert_eq!(&f.image()[0x100..0x104], &[0x78, 0x56, 0x34, 0x12]);

        write32(&mut f, 0, 0x0020_0020);
        write32(&mut f, 0, 0x00d0_00d0);
        write32(&mut f, 0, 0x00ff_00ff);
        assert_eq!(&f.image()[0..4], &[0xff; 4]);
    }

    #[test]
    fn status_command_reports_write_ready_for_both_16bit_chips() {
        let mut f = flash();
        write32(&mut f, 0, 0x0070_0070);
        assert_eq!(read32(&mut f, 0), 0x0080_0080);
    }

    #[test]
    fn read_id_reports_unlocked_block_status() {
        let mut f = flash();
        write32(&mut f, 0, 0x0090_0090);
        assert_eq!(read32(&mut f, 8), 0);
    }

    #[test]
    fn buffered_program_updates_array_after_confirm() {
        let mut f = flash();
        write32(&mut f, 0x100, 0x00e8_00e8);
        assert_eq!(read32(&mut f, 0x100), 0x0080_0080);
        write32(&mut f, 0x100, 0);
        write32(&mut f, 0x100, 0x1234_5678);
        write32(&mut f, 0, 0x00d0_00d0);
        write32(&mut f, 0, 0x00ff_00ff);
        assert_eq!(read32(&mut f, 0x100), 0x1234_5678);
    }

    #[test]
    fn erase_confirm_restores_a_block_to_ones() {
        let mut f = flash();
        f.load(&[0; 16]);
        write32(&mut f, 0, 0x0020_0020);
        write32(&mut f, 0, 0x00d0_00d0);
        write32(&mut f, 0, 0x00ff_00ff);
        assert_eq!(read32(&mut f, 0), 0xffff_ffff);
    }
}
