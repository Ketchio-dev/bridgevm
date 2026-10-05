//! Public-register MSI-X masking and deferred NVMe completion delivery.

use super::*;
use crate::nvme::{ADMIN_OP_IDENTIFY, REG_ACQ, REG_AQA, REG_ASQ, REG_CC, REG_DOORBELL_BASE};
use crate::pcie::{
    CMD_BUS_MASTER, CMD_MEMORY_SPACE, NVME_MSIX_CAP_OFFSET, NVME_MSIX_PBA_OFFSET,
    NVME_MSIX_TABLE_OFFSET, REG_BAR0, REG_COMMAND_STATUS,
};
use crate::platform_virt::FlatGuestRam;

const BAR: u64 = board::PCIE_MMIO_32.base;
const TABLE: u64 = BAR + NVME_MSIX_TABLE_OFFSET as u64;
const PBA: u64 = BAR + NVME_MSIX_PBA_OFFSET as u64;

fn ecam(register: u16) -> u64 {
    board::PCIE_ECAM.base + (1 << 15) + u64::from(register)
}

fn write(p: &mut BridgeVmPcPlatform, m: &mut FlatGuestRam, address: u64, size: u8, value: u64) {
    assert_eq!(
        p.on_mmio(address, MmioOp::Write { size, value }, m),
        MmioOutcome::WriteAck
    );
}

fn read(p: &mut BridgeVmPcPlatform, m: &mut FlatGuestRam, address: u64) -> u64 {
    match p.on_mmio(address, MmioOp::Read { size: 4 }, m) {
        MmioOutcome::ReadValue(value) => value,
        other => panic!("unexpected MMIO result: {other:?}"),
    }
}

fn drain(p: &mut BridgeVmPcPlatform) -> Vec<MsixMessage> {
    let mut messages = Vec::new();
    p.drain_pending_msix_into(&mut messages);
    messages
}

fn message() -> MsixMessage {
    MsixMessage {
        vector: 0,
        address: board::GIC_MSI_FRAME.base + 0x40,
        data: board::GIC_MSI_INTID_BASE,
    }
}

fn setup(function_masked: bool) -> (BridgeVmPcPlatform, FlatGuestRam) {
    let mut p = BridgeVmPcPlatform::new();
    let mut m = FlatGuestRam::new(board::RAM_BASE, 0x10000);
    write(&mut p, &mut m, ecam(REG_BAR0), 4, BAR);
    write(&mut p, &mut m, ecam(REG_BAR0 + 4), 4, 0);
    write(
        &mut p,
        &mut m,
        ecam(REG_COMMAND_STATUS),
        4,
        u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER),
    );
    write(&mut p, &mut m, TABLE, 8, message().address);
    write(&mut p, &mut m, TABLE + 8, 4, u64::from(message().data));
    write(&mut p, &mut m, TABLE + 12, 4, 1);
    write(
        &mut p,
        &mut m,
        ecam(u16::from(NVME_MSIX_CAP_OFFSET) + 2),
        2,
        if function_masked { 0xc008 } else { 0x8008 },
    );
    for (register, size, value) in [
        (REG_AQA, 4, 0x000f_000f),
        (REG_ASQ, 8, board::RAM_BASE),
        (REG_ACQ, 8, board::RAM_BASE + 0x1000),
        (REG_CC, 4, 0x0046_0001),
    ] {
        write(&mut p, &mut m, BAR + register, size, value);
    }
    assert_eq!(read(&mut p, &mut m, PBA), 0);
    assert!(drain(&mut p).is_empty());
    (p, m)
}

fn complete_identify(p: &mut BridgeVmPcPlatform, m: &mut FlatGuestRam) {
    let mut command = [0u8; 64];
    command[0] = ADMIN_OP_IDENTIFY;
    command[2..4].copy_from_slice(&7u16.to_le_bytes());
    command[24..32].copy_from_slice(&(board::RAM_BASE + 0x2000).to_le_bytes());
    command[40..44].copy_from_slice(&1u32.to_le_bytes());
    assert!(m.write_bytes(board::RAM_BASE, &command));
    write(p, m, BAR + REG_DOORBELL_BASE, 4, 1);
    let cqe = m.read_bytes(board::RAM_BASE + 0x1000, 16).unwrap();
    assert_eq!(u16::from_le_bytes(cqe[12..14].try_into().unwrap()), 7);
    assert_eq!(u16::from_le_bytes(cqe[14..16].try_into().unwrap()), 1);
}

#[test]
fn nvme_table_unmask_replays_pending_completion_once() {
    let (mut p, mut m) = setup(false);
    complete_identify(&mut p, &mut m);
    assert!(drain(&mut p).is_empty());
    assert_eq!(read(&mut p, &mut m, PBA), 1);
    write(&mut p, &mut m, TABLE + 12, 4, 1);
    assert_eq!(read(&mut p, &mut m, TABLE + 12), 1);
    assert_eq!(read(&mut p, &mut m, PBA), 1);
    assert!(
        drain(&mut p).is_empty(),
        "masked writes and reads stay quiet"
    );

    write(&mut p, &mut m, TABLE + 12, 4, 0);
    assert_eq!(drain(&mut p), [message()], "BAR unmask must replay PBA");
    assert_eq!(read(&mut p, &mut m, PBA), 0);
    write(&mut p, &mut m, TABLE + 12, 4, 0);
    assert_eq!(read(&mut p, &mut m, TABLE + 12), 0);
    assert!(
        drain(&mut p).is_empty(),
        "no second message without new work"
    );
}

#[test]
fn nvme_function_mask_blocks_table_unmask_until_config_unmask() {
    let (mut p, mut m) = setup(true);
    complete_identify(&mut p, &mut m);
    assert_eq!(read(&mut p, &mut m, PBA), 1);
    write(&mut p, &mut m, TABLE + 12, 4, 0);
    assert!(drain(&mut p).is_empty());
    assert_eq!(read(&mut p, &mut m, PBA), 1);
    write(
        &mut p,
        &mut m,
        ecam(u16::from(NVME_MSIX_CAP_OFFSET) + 2),
        2,
        0x8008,
    );
    assert_eq!(drain(&mut p), [message()]);
    assert_eq!(read(&mut p, &mut m, PBA), 0);
    assert!(drain(&mut p).is_empty());
}

#[test]
fn nvme_table_unmask_without_pending_completion_stays_quiet() {
    let (mut p, mut m) = setup(false);
    write(&mut p, &mut m, TABLE + 12, 4, 0);
    assert_eq!(read(&mut p, &mut m, PBA), 0);
    assert!(drain(&mut p).is_empty());
}

#[test]
fn nvme_unmasked_completion_delivers_without_pending_replay() {
    let (mut p, mut m) = setup(false);
    write(&mut p, &mut m, TABLE + 12, 4, 0);
    complete_identify(&mut p, &mut m);
    assert_eq!(drain(&mut p), [message()]);
    assert_eq!(read(&mut p, &mut m, PBA), 0);
    assert!(drain(&mut p).is_empty());
}
