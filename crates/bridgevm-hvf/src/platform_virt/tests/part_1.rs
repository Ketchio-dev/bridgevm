//! Split test module.

use super::super::*;
use super::helpers::*;
use crate::acpi::ACPI_TABLE_FILE;
use crate::acpi::ACPI_TPM_LOG_FILE;
use crate::dtb::VirtFdtConfig;
use crate::fwcfg::GuestMemoryMut;
use crate::fwcfg::DMA_CTL_READ;
use crate::fwcfg::DMA_CTL_SELECT;
use crate::fwcfg::KEY_SIGNATURE;
use crate::machine;
use crate::pcie::PcieMmioTarget;
use crate::pcie::NVME_BDF;
use crate::tpm_ppi::build_qemu_fw_cfg_tpm_config;
use crate::tpm_ppi::TpmPpiStats;
use crate::tpm_ppi::TPM_PPI_FW_CFG_FILE;
use crate::tpm_tis::TpmTisStats;
use crate::virtio_blk::INSTALLER_ISO_SLOT;

#[test]
fn tpm_presence_wires_mmio_and_acpi_as_one_contract() {
    let mut devices = VirtPlatformDeviceConfig::default();
    devices.tpm_tis_present = true;
    let mut p = VirtPlatform::new_with_config_and_tpm_backend(
        VirtPlatformConfig {
            fdt: VirtFdtConfig::default(),
            devices,
        },
        Some(Box::new(TestTpmBackend)),
    );
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);

    assert_eq!(
        p.on_mmio(
            machine::TPM_TIS.base + crate::tpm_tis::REG_DID_VID,
            MmioOp::Read { size: 4 },
            &mut mem,
        ),
        MmioOutcome::ReadValue(0x0001_1014)
    );
    assert_eq!(p.tpm_tis_stats(), Some(TpmTisStats::default()));

    let (selector, size) = fw_cfg_file_entry(&mut p, ACPI_TABLE_FILE.as_bytes());
    p.fw_cfg.select(selector);
    let acpi_tables = p.fw_cfg.read_data(size);
    assert!(acpi_tables.windows(4).any(|bytes| bytes == b"TPM0"));
    assert!(acpi_tables.windows(8).any(|bytes| bytes == b"MSFT0101"));
    let (_, tpm_log_size) = fw_cfg_file_entry(&mut p, ACPI_TPM_LOG_FILE.as_bytes());
    assert_eq!(tpm_log_size, crate::acpi::TPM_LOG_AREA_MINIMUM_SIZE);
    let (ppi_config_selector, ppi_config_size) =
        fw_cfg_file_entry(&mut p, TPM_PPI_FW_CFG_FILE.as_bytes());
    assert_eq!(ppi_config_size, crate::tpm_ppi::TPM_PPI_FW_CFG_CONFIG_SIZE);
    p.fw_cfg.select(ppi_config_selector);
    assert_eq!(
        p.fw_cfg.read_data(ppi_config_size),
        build_qemu_fw_cfg_tpm_config(machine::TPM_PPI.base as u32)
    );
    assert_eq!(
        p.on_mmio(
            machine::TPM_PPI.base + crate::tpm_ppi::PPRQ_OFFSET as u64,
            MmioOp::Write { size: 4, value: 23 },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            machine::TPM_PPI.base + crate::tpm_ppi::PPRQ_OFFSET as u64,
            MmioOp::Read { size: 4 },
            &mut mem,
        ),
        MmioOutcome::ReadValue(23)
    );
    assert_eq!(
        p.tpm_ppi_stats(),
        Some(TpmPpiStats {
            reads: 1,
            writes: 1,
            rejected_accesses: 0,
        })
    );
}

#[test]
fn disabled_tpm_omits_all_tpm_fw_cfg_discovery_files() {
    let mut p = platform();

    assert_eq!(
        find_fw_cfg_file_entry(&mut p, ACPI_TPM_LOG_FILE.as_bytes()),
        None
    );
    assert_eq!(
        find_fw_cfg_file_entry(&mut p, TPM_PPI_FW_CFG_FILE.as_bytes()),
        None
    );
}

#[test]
fn pending_irq_drains_preserve_internal_capacity() {
    let mut p = platform();
    let message = crate::msix::MsixMessage {
        vector: 7,
        address: machine::GIC_ITS.base + 0x40,
        data: 42,
    };

    p.pending_msix.reserve(8);
    p.pending_msix.push(message);
    let msix_capacity = p.pending_msix.capacity();
    assert_eq!(p.take_pending_msix(), vec![message]);
    assert!(p.take_pending_msix().is_empty());
    assert_eq!(p.pending_msix.capacity(), msix_capacity);

    p.pending_spi_levels.reserve(8);
    p.pending_spi_levels.push((machine::spi_to_intid(7), true));
    let spi_capacity = p.pending_spi_levels.capacity();
    assert_eq!(
        p.take_pending_spi_levels(),
        vec![(machine::spi_to_intid(7), true)]
    );
    assert!(p.take_pending_spi_levels().is_empty());
    assert_eq!(p.pending_spi_levels.capacity(), spi_capacity);
}

#[test]
fn pending_irq_drain_into_reuses_caller_capacity() {
    let mut p = platform();
    let message = crate::msix::MsixMessage {
        vector: 7,
        address: machine::GIC_ITS.base + 0x40,
        data: 42,
    };

    p.pending_msix.reserve(8);
    p.pending_msix.push(message);
    let msix_internal_capacity = p.pending_msix.capacity();
    let mut msix_out = Vec::with_capacity(8);
    let msix_out_capacity = msix_out.capacity();
    let msix_out_ptr = msix_out.as_ptr();
    p.drain_pending_msix_into(&mut msix_out);
    assert_eq!(msix_out, vec![message]);
    assert_eq!(msix_out.capacity(), msix_out_capacity);
    assert_eq!(msix_out.as_ptr(), msix_out_ptr);
    assert_eq!(p.pending_msix.capacity(), msix_internal_capacity);
    msix_out.clear();
    p.drain_pending_msix_into(&mut msix_out);
    assert!(msix_out.is_empty());
    assert_eq!(msix_out.capacity(), msix_out_capacity);

    p.pending_spi_levels.reserve(8);
    p.pending_spi_levels.push((machine::spi_to_intid(7), true));
    let spi_internal_capacity = p.pending_spi_levels.capacity();
    let mut spi_out = Vec::with_capacity(8);
    let spi_out_capacity = spi_out.capacity();
    let spi_out_ptr = spi_out.as_ptr();
    p.drain_pending_spi_levels_into(&mut spi_out);
    assert_eq!(spi_out, vec![(machine::spi_to_intid(7), true)]);
    assert_eq!(spi_out.capacity(), spi_out_capacity);
    assert_eq!(spi_out.as_ptr(), spi_out_ptr);
    assert_eq!(p.pending_spi_levels.capacity(), spi_internal_capacity);
    spi_out.clear();
    p.drain_pending_spi_levels_into(&mut spi_out);
    assert!(spi_out.is_empty());
    assert_eq!(spi_out.capacity(), spi_out_capacity);
}

#[test]
fn on_mmio_with_post_drain_reports_setup_input_attempts() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);

    let (outcome, post_drain) =
        p.on_mmio_with_post_drain(machine::UART.base, MmioOp::Read { size: 4 }, &mut mem);
    assert!(matches!(outcome, MmioOutcome::ReadValue(_)));
    assert!(post_drain.xhci_setup_input_attempted());

    let (outcome, post_drain) = p.on_mmio_with_post_drain(
        machine::RAM_BASE - 0x1000,
        MmioOp::Read { size: 4 },
        &mut mem,
    );
    assert_eq!(outcome, MmioOutcome::Unmapped);
    assert!(!post_drain.xhci_setup_input_attempted());
}

#[test]
fn on_mmio_with_post_drain_skips_setup_input_for_xhci_bar0() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    let xhci_base = machine::PCIE_MMIO_32.base + 0x2_0000;

    for (reg, size, value) in [
        (crate::pcie::REG_BAR0, 4, xhci_base),
        (
            crate::pcie::REG_COMMAND_STATUS,
            2,
            u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
        ),
    ] {
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(crate::pcie::XHCI_BDF.1, crate::pcie::XHCI_BDF.2, reg),
                MmioOp::Write { size, value },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
    }

    let (outcome, post_drain) =
        p.on_mmio_with_post_drain(xhci_base, MmioOp::Read { size: 4 }, &mut mem);
    assert!(matches!(outcome, MmioOutcome::ReadValue(_)));
    assert!(!post_drain.xhci_setup_input_attempted());
}

#[test]
fn dtb_is_generated_and_well_formed() {
    let p = platform();
    let dtb = p.dtb();
    assert_eq!(
        u32::from_be_bytes([dtb[0], dtb[1], dtb[2], dtb[3]]),
        0xd00d_feed
    );
}

#[test]
fn memory_layout_is_consistent() {
    let p = platform();
    let l = p.memory_layout();
    assert_eq!(l.flash_code.base, 0x0);
    assert_eq!(l.flash_vars.base, 0x0400_0000);
    assert_eq!(l.ram.base, machine::RAM_BASE);
    assert_eq!(l.dtb_load, machine::RAM_BASE);
    // Flash and RAM must not overlap.
    assert!(!l.flash_vars.overlaps(&l.ram));
    // The DTB must fit inside RAM.
    assert!(l.ram.contains(l.dtb_load));
    assert!(p.dtb().len() as u64 <= l.ram.size);
}

#[test]
fn mmio_routes_fw_cfg_signature_via_the_platform() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    // Select SIGNATURE through the platform's MMIO entry point...
    let ack = p.on_mmio(
        machine::FW_CFG.base + REG_SELECTOR,
        MmioOp::Write {
            size: 2,
            value: u64::from(KEY_SIGNATURE),
        },
        &mut mem,
    );
    assert_eq!(ack, MmioOutcome::WriteAck);
    // ...then a 4-byte data read returns the little-endian CPU value for
    // the byte stream "QEMU".
    let v = p.on_mmio(
        machine::FW_CFG.base + REG_DATA,
        MmioOp::Read { size: 4 },
        &mut mem,
    );
    assert_eq!(v, MmioOutcome::ReadValue(0x554d_4551));
}

#[test]
fn mmio_fw_cfg_dma_transfers_through_guest_ram() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
    let ctrl = machine::RAM_BASE;
    let dst = machine::RAM_BASE + 0x80;
    // Build a FWCfgDmaAccess (big-endian) that selects SIGNATURE and reads
    // 4 bytes into `dst`.
    let control: u32 = (u32::from(KEY_SIGNATURE) << 16) | DMA_CTL_SELECT | DMA_CTL_READ;
    let mut blob = Vec::new();
    blob.extend_from_slice(&control.to_be_bytes());
    blob.extend_from_slice(&4u32.to_be_bytes());
    blob.extend_from_slice(&dst.to_be_bytes());
    mem.write_bytes(ctrl, &blob);
    // Writing the control-structure address to the DMA register runs it. The
    // register is big-endian, so the firmware stores the byte-swapped address.
    let ack = p.on_mmio(
        machine::FW_CFG.base + REG_DMA,
        MmioOp::Write {
            size: 8,
            value: ctrl.swap_bytes(),
        },
        &mut mem,
    );
    assert_eq!(ack, MmioOutcome::WriteAck);
    assert_eq!(mem.read_bytes(dst, 4).unwrap(), b"QEMU");
}

#[test]
fn mmio_classifies_known_and_unmapped_addresses() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    // GIC is mapped in the machine map but not yet modelled.
    assert_eq!(
        p.on_mmio(machine::GIC_DIST.base, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::KnownUnimplemented("gic-dist")
    );
    // A hole between GPIO and the virtio block.
    assert_eq!(
        p.on_mmio(0x0905_0000, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::Unmapped
    );
}

#[test]
fn pcie_host_bridge_and_empty_slots() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    // 00:00.0 is the host bridge (vendor 0x1b36 / device 0x0008).
    assert_eq!(
        p.on_mmio(machine::PCIE_ECAM.base, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0x0008_1b36)
    );
    assert_eq!(
        p.on_mmio(
            machine::PCIE_ECAM.base + (4 << 15),
            MmioOp::Read { size: 4 },
            &mut mem
        ),
        MmioOutcome::ReadValue(0xFFFF_FFFF)
    );
}

#[test]
fn platform_device_disable_omits_xhci_from_pci_and_mmio_surfaces() {
    let mut p = platform_with_devices(VirtPlatformDeviceConfig {
        xhci_present: false,
        ..VirtPlatformDeviceConfig::default()
    });
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    let xhci_base = machine::PCIE_MMIO_32.base + 0x2_0000;

    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(crate::pcie::XHCI_BDF.1, crate::pcie::XHCI_BDF.2, 0),
            MmioOp::Read { size: 4 },
            &mut mem
        ),
        MmioOutcome::ReadValue(crate::pcie::NO_DEVICE)
    );
    for (reg, value) in [
        (crate::pcie::REG_BAR0, xhci_base),
        (crate::pcie::REG_BAR0 + 4, 0),
        (
            crate::pcie::REG_COMMAND_STATUS,
            u64::from(crate::pcie::CMD_MEMORY_SPACE),
        ),
    ] {
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(crate::pcie::XHCI_BDF.1, crate::pcie::XHCI_BDF.2, reg),
                MmioOp::Write {
                    size: if reg == crate::pcie::REG_COMMAND_STATUS {
                        2
                    } else {
                        4
                    },
                    value,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
    }

    assert_eq!(p.pcie_mmio_target(xhci_base), None);
    assert_eq!(
        p.on_mmio(xhci_base, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::KnownUnimplemented("pcie-mmio-32")
    );
    assert!(!String::from_utf8_lossy(p.dtb()).contains("xhci"));
}

#[test]
fn platform_device_disable_omits_virtio_iso_from_dtb_pci_and_mmio_surfaces() {
    let mut p = platform_with_devices(VirtPlatformDeviceConfig {
        virtio_boot_media_present: false,
        legacy_virtio_mmio_present: false,
        ..VirtPlatformDeviceConfig::default()
    });
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    let pci_bar = machine::PCIE_MMIO_32.base + 0x8_0000;
    let legacy_slot = machine::virtio_mmio_slot(INSTALLER_ISO_SLOT);
    let dtb_body = String::from_utf8_lossy(p.dtb());

    assert!(!dtb_body.contains("virtio_mmio@"));
    assert_eq!(find_fw_cfg_file_entry(&mut p, b"bootorder"), None);
    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::VIRTIO_BLK_BDF.1,
                crate::pcie::VIRTIO_BLK_BDF.2,
                0
            ),
            MmioOp::Read { size: 4 },
            &mut mem
        ),
        MmioOutcome::ReadValue(crate::pcie::NO_DEVICE)
    );
    program_virtio_blk_bar4(&mut p, &mut mem, pci_bar);
    assert_eq!(p.pcie_mmio_target(pci_bar), None);
    assert_eq!(
        p.on_mmio(legacy_slot.base, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::Unmapped
    );
    assert_eq!(p.virtio_iso_stats(), None);
    assert_eq!(p.pci_boot_media_stats(), None);
}

#[test]
fn pcie_nvme_liveness_separates_bar_command_mmio_cc_and_admin_doorbell() {
    const ASQ: u64 = machine::RAM_BASE + 0x1000;
    const ACQ: u64 = machine::RAM_BASE + 0x2000;
    const DATA: u64 = machine::RAM_BASE + 0x3000;

    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x8000);

    // Given: the NVMe endpoint is advertised but untouched.
    let initial = p.nvme_pcie_liveness();
    assert!(initial.nvme_advertised);
    assert!(!initial.nvme_ecam_touched);
    assert!(!initial.nvme_bar0_assigned);
    assert!(!initial.nvme_command_memory_enabled);
    assert!(!initial.nvme_command_bus_master_enabled);
    assert!(!initial.nvme_mmio_reached);
    assert!(!initial.nvme_cc_enabled);
    assert!(!initial.nvme_admin_doorbell_rung);

    // When: firmware assigns NVMe BAR0 but leaves command memory disabled.
    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(1, 0, crate::pcie::REG_BAR0),
            MmioOp::Write {
                size: 4,
                value: machine::PCIE_MMIO_32.base,
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );

    // Then: BAR assignment is visible without claiming MMIO reachability.
    let bar_only = p.nvme_pcie_liveness();
    assert!(bar_only.nvme_ecam_touched);
    assert!(bar_only.nvme_bar0_assigned);
    assert!(!bar_only.nvme_command_memory_enabled);
    assert!(!bar_only.nvme_command_bus_master_enabled);
    assert!(!bar_only.nvme_mmio_reached);
    assert_eq!(p.pcie_mmio_target(machine::PCIE_MMIO_32.base), None);

    // When: only NVMe command memory is enabled.
    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(1, 0, crate::pcie::REG_COMMAND_STATUS),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_MEMORY_SPACE),
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );

    // Then: MMIO decode is enabled while bus-master is still reported apart.
    let memory_only = p.nvme_pcie_liveness();
    assert!(memory_only.nvme_command_memory_enabled);
    assert!(!memory_only.nvme_command_bus_master_enabled);
    assert_eq!(
        p.pcie_mmio_target(machine::PCIE_MMIO_32.base),
        Some(PcieMmioTarget {
            bdf: NVME_BDF,
            bar_index: 0,
            offset: 0,
        })
    );
    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + crate::nvme::REG_VS,
            MmioOp::Read { size: 4 },
            &mut mem,
        ),
        MmioOutcome::ReadValue(u64::from(crate::nvme::NVME_VERSION_1_4_0))
    );
    let mmio_read = p.nvme_pcie_liveness();
    assert!(mmio_read.nvme_mmio_reached);
    assert!(!mmio_read.nvme_cc_enabled);
    assert!(!mmio_read.nvme_admin_doorbell_rung);

    // When: bus-master is added and the admin queue is used.
    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(1, 0, crate::pcie::REG_COMMAND_STATUS),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    enable_nvme_controller(&mut p, &mut mem, ASQ, ACQ);
    let identify_controller = encode_nvme_sqe(0x06, 11, 0, DATA, 0x01, 0, 0);
    submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &identify_controller);

    // Then: liveness distinguishes bus-master, CC enable, and doorbell.
    let live = p.nvme_pcie_liveness();
    assert!(live.nvme_command_bus_master_enabled);
    assert!(live.nvme_cc_enabled);
    assert!(live.nvme_admin_doorbell_rung);
}

#[test]
fn pcie_nvme_bar0_doorbell_processes_admin_identify() {
    const ASQ: u64 = machine::RAM_BASE + 0x1000;
    const ACQ: u64 = machine::RAM_BASE + 0x2000;
    const DATA: u64 = machine::RAM_BASE + 0x3000;

    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x8000);
    program_nvme_bar0(&mut p, &mut mem);

    enable_nvme_controller(&mut p, &mut mem, ASQ, ACQ);
    p.nvme_completion_scratch.reserve(4);
    let completion_scratch_capacity = p.nvme_completion_scratch.capacity();
    let completion_scratch_ptr = p.nvme_completion_scratch.as_ptr();

    let identify_controller = encode_nvme_sqe(0x06, 7, 0, DATA, 0x01, 0, 0);
    submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &identify_controller);
    assert!(p.nvme_completion_scratch.is_empty());
    assert_eq!(
        p.nvme_completion_scratch.capacity(),
        completion_scratch_capacity
    );
    assert_eq!(p.nvme_completion_scratch.as_ptr(), completion_scratch_ptr);

    let identify = mem.read_bytes(DATA, 4096).unwrap();
    assert_eq!(u16::from_le_bytes([identify[0], identify[1]]), 0x1b36);
    assert!(identify[24..64].starts_with(b"BridgeVM NVMe"));

    let completion = mem.read_bytes(ACQ, 16).unwrap();
    assert_eq!(u16::from_le_bytes([completion[12], completion[13]]), 7);
    let status = u16::from_le_bytes([completion[14], completion[15]]);
    assert_eq!(status & 0x1, 1, "phase tag must be set");
    assert_eq!(status >> 1, 0, "identify must complete successfully");
}

#[test]
fn pcie_nvme_msix_table_completion_queues_a_message() {
    const ASQ: u64 = machine::RAM_BASE + 0x1000;
    const ACQ: u64 = machine::RAM_BASE + 0x2000;
    const DATA: u64 = machine::RAM_BASE + 0x3000;
    const MSI_ADDRESS: u64 = machine::GIC_ITS.base + 0x40;
    const MSI_DATA: u32 = 35;

    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x8000);
    program_nvme_bar0(&mut p, &mut mem);
    enable_nvme_msix_vector0(&mut p, &mut mem, MSI_ADDRESS, MSI_DATA);
    enable_nvme_controller(&mut p, &mut mem, ASQ, ACQ);

    let identify_controller = encode_nvme_sqe(0x06, 9, 0, DATA, 0x01, 0, 0);
    submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &identify_controller);

    assert_eq!(
        p.take_pending_msix(),
        vec![crate::msix::MsixMessage {
            vector: 0,
            address: MSI_ADDRESS,
            data: MSI_DATA,
        }]
    );
}
