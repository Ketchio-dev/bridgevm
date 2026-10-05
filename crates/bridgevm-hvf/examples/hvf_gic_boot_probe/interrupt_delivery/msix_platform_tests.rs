//! Real NVMe Identify DMA/completion, with no VM or native interrupt call.
use super::*;
use bridgevm_hvf::platform_virt::FlatGuestRam;
use bridgevm_hvf::{nvme, pcie};

#[test]
fn failed_valid_nvme_completion_is_consumed_without_regeneration_and_stops_caller() {
    const ASQ: u64 = machine::RAM_BASE + 0x1000;
    const ACQ: u64 = machine::RAM_BASE + 0x2000;
    const DATA: u64 = machine::RAM_BASE + 0x3000;
    let address = machine::GIC_MSI_FRAME.base + 0x40;
    let data = machine::GIC_MSI_INTID_BASE;
    let mut platform = VirtPlatform::new(VirtFdtConfig::default());
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x8000);
    let cfg = machine::PCIE_ECAM.base + (1 << 15);
    for (register, size, value) in [
        (pcie::REG_BAR0, 4, machine::PCIE_MMIO_32.base),
        (
            pcie::REG_COMMAND_STATUS,
            2,
            u64::from(pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER),
        ),
        (u16::from(pcie::NVME_MSIX_CAP_OFFSET) + 2, 2, 0x8000),
    ] {
        assert_eq!(
            platform.on_mmio(
                cfg + u64::from(register),
                MmioOp::Write { size, value },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
    }
    for (register, size, value) in [
        (u64::from(pcie::NVME_MSIX_TABLE_OFFSET), 8, address),
        (
            u64::from(pcie::NVME_MSIX_TABLE_OFFSET) + 8,
            4,
            u64::from(data),
        ),
        (u64::from(pcie::NVME_MSIX_TABLE_OFFSET) + 12, 4, 0),
        (nvme::REG_AQA, 4, (3 << 16) | 3),
        (nvme::REG_ASQ, 8, ASQ),
        (nvme::REG_ACQ, 8, ACQ),
        (nvme::REG_CC, 4, 1),
    ] {
        assert_eq!(
            platform.on_mmio(
                machine::PCIE_MMIO_32.base + register,
                MmioOp::Write { size, value },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
    }
    let mut sqe = [0u8; 64];
    sqe[0..4].copy_from_slice(&(0x06u32 | (9 << 16)).to_le_bytes());
    sqe[24..32].copy_from_slice(&DATA.to_le_bytes());
    sqe[40..44].copy_from_slice(&1u32.to_le_bytes());
    assert!(mem.write_bytes(ASQ, &sqe));
    assert_eq!(
        platform.on_mmio(
            machine::PCIE_MMIO_32.base + nvme::REG_DOORBELL_BASE,
            MmioOp::Write { size: 4, value: 1 },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    let cq = mem.read_bytes(ACQ, 16).unwrap();
    assert_eq!(u16::from_le_bytes([cq[12], cq[13]]), 9);
    assert_eq!(u16::from_le_bytes([cq[14], cq[15]]), 1);
    assert!(mem
        .read_bytes(DATA + 24, 40)
        .unwrap()
        .starts_with(b"BridgeVM NVMe"));
    let mut stats = RunLoopDrainStats::new(false);
    platform.drain_pending_msix_into(&mut stats.pending_msix_scratch);
    assert_eq!(
        stats.pending_msix_scratch,
        [MsixMessage {
            vector: 0,
            address,
            data
        }]
    );
    let pending = Ok(PendingDrainDelivery {
        context: DrainContext::data_abort(7, 0x1234),
        spi: DeliveryCounts::default(),
    });
    let mut calls = Vec::new();
    let result = complete_prepared_interrupt_delivery(pending, |pending| {
        stats.complete_pending_delivery_with(pending, |messages| {
            deliver_msix_messages(messages, |message| {
                calls.push(message);
                0xfae9_4001u32 as i32
            })
        })
    });
    assert_eq!(
        calls,
        [MsixMessage {
            vector: 0,
            address,
            data
        }]
    );
    assert_eq!(
        platform.on_mmio(
            machine::PCIE_MMIO_32.base + u64::from(pcie::NVME_MSIX_PBA_OFFSET),
            MmioOp::Read { size: 8 },
            &mut mem
        ),
        MmioOutcome::ReadValue(0)
    );
    assert!(platform.take_pending_msix().is_empty());
    assert!(
        result.is_err(),
        "valid NVMe completion consumed, provider error lost, caller allowed continuation"
    );
    let (mut fatal, mut pc) = (false, 0);
    result.err().unwrap().stop_primary(&mut fatal, &mut pc);
    assert!(fatal);
    assert_eq!(pc, 0x1234);
    assert_eq!(
        (stats.msix.drained, stats.msix.success, stats.msix.failure),
        (1, 0, 1)
    );
}
