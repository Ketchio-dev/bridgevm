use crate::dtb::VirtFdtConfig;
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::{FlatGuestRam, MmioOp, MmioOutcome, VirtPlatform};
use crate::{machine, pcie};

const XHCI_BAR0: u64 = machine::PCIE_MMIO_32.base + 0x2_0000;
const RAM_LEN: usize = 0x10_000;
const COMMAND_RING: u64 = machine::RAM_BASE + 0x1000;
const ERST: u64 = machine::RAM_BASE + 0x2000;
const EVENT_RING: u64 = machine::RAM_BASE + 0x3000;
const DCBAA: u64 = machine::RAM_BASE + 0x4000;
const TRB_TYPE_ENABLE_SLOT: u32 = 9;
const TRB_TYPE_ADDRESS_DEVICE: u32 = 11;
const TRB_TYPE_COMMAND_COMPLETION_EVENT: u32 = 33;
const COMPLETION_CODE_SUCCESS: u32 = 1;
const ENABLE_SLOT_ID: u32 = 1;
const ADDRESS_DEVICE_SLOT_ID: u32 = 6;

fn pcie_cfg_gpa(device: u8, function: u8, reg: u16) -> u64 {
    machine::PCIE_ECAM.base
        + (u64::from(device) << 15)
        + (u64::from(function) << 12)
        + u64::from(reg)
}

fn program_xhci_bar0(platform: &mut VirtPlatform, mem: &mut FlatGuestRam) {
    assert_eq!(
        platform.on_mmio(
            pcie_cfg_gpa(pcie::XHCI_BDF.1, pcie::XHCI_BDF.2, pcie::REG_BAR0),
            MmioOp::Write {
                size: 4,
                value: XHCI_BAR0,
            },
            mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        platform.on_mmio(
            pcie_cfg_gpa(pcie::XHCI_BDF.1, pcie::XHCI_BDF.2, pcie::REG_COMMAND_STATUS),
            MmioOp::Write {
                size: 2,
                value: u64::from(pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER),
            },
            mem,
        ),
        MmioOutcome::WriteAck
    );
}

fn read_u32(mem: &FlatGuestRam, gpa: u64) -> u32 {
    u32::from_le_bytes(mem.read_bytes(gpa, 4).unwrap().try_into().unwrap())
}

fn write_command_trb(mem: &mut FlatGuestRam, command_control: u32) {
    let command = [0u32, 0, 0, command_control];
    for (index, dword) in command.iter().enumerate() {
        assert!(mem.write_bytes(
            COMMAND_RING + u64::try_from(index).unwrap() * 4,
            &dword.to_le_bytes()
        ));
    }
}

fn write_event_ring_table(mem: &mut FlatGuestRam) {
    assert!(mem.write_bytes(ERST, &EVENT_RING.to_le_bytes()));
    assert!(mem.write_bytes(ERST + 8, &16u32.to_le_bytes()));
}

fn command_control(trb_type: u32, slot_id: u32) -> u32 {
    (slot_id << 24) | (trb_type << 10) | 1
}

fn assert_success_completion(mem: &FlatGuestRam, expected_slot_id: u32) {
    assert_eq!(read_u32(mem, EVENT_RING), low_u32(COMMAND_RING));
    assert_eq!(read_u32(mem, EVENT_RING + 4), high_u32(COMMAND_RING));
    assert_eq!(read_u32(mem, EVENT_RING + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = read_u32(mem, EVENT_RING + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_COMMAND_COMPLETION_EVENT);
    assert_eq!(control & 1, 1);
    assert_eq!(control >> 24, expected_slot_id);
}

fn low_u32(value: u64) -> u32 {
    u32::try_from(value & 0xffff_ffff).unwrap()
}

fn high_u32(value: u64) -> u32 {
    u32::try_from(value >> 32).unwrap()
}

#[test]
fn xhci_posts_enable_slot_command_completion_when_doorbell_zero_rings() {
    // Given: a guest-owned command ring with one Enable Slot TRB and an event ring.
    let mut platform = VirtPlatform::new(VirtFdtConfig::default());
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, RAM_LEN);
    program_xhci_bar0(&mut platform, &mut mem);

    write_command_trb(
        &mut mem,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    write_event_ring_table(&mut mem);

    let mut write_reg = |offset: u64, size: u8, value: u64| {
        assert_eq!(
            platform.on_mmio(XHCI_BAR0 + offset, MmioOp::Write { size, value }, &mut mem,),
            MmioOutcome::WriteAck
        );
    };
    write_reg(0x58, 8, COMMAND_RING | 1);
    write_reg(0x70, 8, DCBAA);
    write_reg(0x78, 4, 1);
    write_reg(0x1020, 4, 2);
    write_reg(0x1028, 4, 1);
    write_reg(0x1030, 8, ERST);
    write_reg(0x1038, 8, EVENT_RING);

    // When: software rings the host-controller command doorbell.
    write_reg(0x2000, 4, 0);

    // Then: the primary event ring receives a successful Command Completion Event.
    assert_success_completion(&mem, ENABLE_SLOT_ID);
    assert_eq!(
        platform.on_mmio(XHCI_BAR0 + 0x1020, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(3)
    );
}

#[test]
fn xhci_posts_address_device_command_completion_when_doorbell_zero_rings() {
    // Given: a guest-owned Address Device command TRB and an event ring.
    let mut platform = VirtPlatform::new(VirtFdtConfig::default());
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, RAM_LEN);
    program_xhci_bar0(&mut platform, &mut mem);
    write_command_trb(
        &mut mem,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ADDRESS_DEVICE_SLOT_ID),
    );
    write_event_ring_table(&mut mem);

    let mut write_reg = |offset: u64, size: u8, value: u64| {
        assert_eq!(
            platform.on_mmio(XHCI_BAR0 + offset, MmioOp::Write { size, value }, &mut mem,),
            MmioOutcome::WriteAck
        );
    };
    write_reg(0x58, 8, COMMAND_RING | 1);
    write_reg(0x70, 8, DCBAA);
    write_reg(0x78, 4, 1);
    write_reg(0x1020, 4, 2);
    write_reg(0x1028, 4, 1);
    write_reg(0x1030, 8, ERST);
    write_reg(0x1038, 8, EVENT_RING);

    // When: software rings BAR0 doorbell 0 at offset 0x2000.
    write_reg(0x2000, 4, 0);

    // Then: the event ring receives a successful completion for the requested slot.
    assert_success_completion(&mem, ADDRESS_DEVICE_SLOT_ID);
}
