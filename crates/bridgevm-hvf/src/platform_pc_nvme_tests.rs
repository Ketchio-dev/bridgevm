use super::*;
use crate::nvme::{ADMIN_OP_IDENTIFY, REG_ACQ, REG_AQA, REG_ASQ, REG_CC, REG_DOORBELL_BASE};
use crate::pcie::{CMD_BUS_MASTER, CMD_MEMORY_SPACE, REG_BAR0, REG_COMMAND_STATUS};
use crate::platform_virt::FlatGuestRam;

fn ecam(register: u16) -> u64 {
    board::PCIE_ECAM.base + (1 << 15) + u64::from(register)
}

fn write(platform: &mut BridgeVmPcPlatform, mem: &mut FlatGuestRam, gpa: u64, value: u64) {
    assert_eq!(
        platform.on_mmio(gpa, MmioOp::Write { size: 4, value }, mem),
        MmioOutcome::WriteAck
    );
}

#[test]
fn nvme_doorbell_processes_an_admin_queue_against_guest_ram() {
    let mut platform = BridgeVmPcPlatform::new();
    let mut mem = FlatGuestRam::new(board::RAM_BASE, 0x10000);
    let bar = board::PCIE_MMIO_32.base;
    let asq = board::RAM_BASE;
    let acq = board::RAM_BASE + 0x1000;
    let identify = board::RAM_BASE + 0x2000;
    write(&mut platform, &mut mem, ecam(REG_BAR0), bar);
    write(&mut platform, &mut mem, ecam(REG_BAR0 + 4), 0);
    write(
        &mut platform,
        &mut mem,
        ecam(REG_COMMAND_STATUS),
        u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER),
    );

    let mut command = [0u8; 64];
    command[0] = ADMIN_OP_IDENTIFY;
    command[2..4].copy_from_slice(&1u16.to_le_bytes());
    command[24..32].copy_from_slice(&identify.to_le_bytes());
    command[40..44].copy_from_slice(&1u32.to_le_bytes());
    assert!(mem.write_bytes(asq, &command));

    for (register, size, value) in [
        (REG_AQA, 4, 0x000f_000f),
        (REG_ASQ, 8, asq),
        (REG_ACQ, 8, acq),
        (REG_CC, 4, 1),
        (REG_DOORBELL_BASE, 4, 1),
    ] {
        assert_eq!(
            platform.on_mmio(bar + register, MmioOp::Write { size, value }, &mut mem,),
            MmioOutcome::WriteAck
        );
    }

    let completion = mem.read_bytes(acq, 16).expect("admin completion");
    assert_eq!(
        u16::from_le_bytes(completion[12..14].try_into().unwrap()),
        1
    );
    assert_eq!(
        u16::from_le_bytes(completion[14..16].try_into().unwrap()),
        1
    );
    let identify_data = mem.read_bytes(identify, 4096).expect("identify controller");
    assert_eq!(&identify_data[24..32], b"BridgeVM");
}
