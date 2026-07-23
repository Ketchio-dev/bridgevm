//! Tests for the Windows-on-Arm constants and vector vocabulary.

use super::helpers::*;
use crate::windows_arm::*;
use crate::*;

#[test]
fn low_vector_remap_target_requires_populated_non_diagnostic_current_el_spx_slot() {
    let recommendation = |word, base_physical_address| WindowsArmUefiVectorBaseRecommendation {
        base_virtual_address: WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA,
        base_physical_address,
        current_el_spx_sync_instruction_word: word,
        current_el_spx_sync_instruction_hint: "unit-test",
        reason: "unit-test",
    };

    assert!(recommendation(
        Some(0xd503_207f),
        Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
    )
    .is_populated_low_vector_remap_target());
    assert!(
        !recommendation(Some(0), Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA))
            .is_populated_low_vector_remap_target()
    );
    assert!(!recommendation(
        Some(0xffff_ffff),
        Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
    )
    .is_populated_low_vector_remap_target());
    assert!(!recommendation(
        Some(AARCH64_HVC_0_INSTRUCTION),
        Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
    )
    .is_populated_low_vector_remap_target());
    assert!(!recommendation(
        Some(AARCH64_HVC_1_INSTRUCTION),
        Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
    )
    .is_populated_low_vector_remap_target());
    assert!(!recommendation(
        Some(AARCH64_ERET_INSTRUCTION),
        Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
    )
    .is_populated_low_vector_remap_target());
    assert!(!recommendation(Some(0xd503_207f), None).is_populated_low_vector_remap_target());
}

#[test]
fn windows_11_arm_platform_description_probe_is_fdt_first_and_metadata_safe() {
    let probe = probe_windows_11_arm_platform_description(WindowsArmPlatformDescriptionOptions {
        guest_ram_bytes: 8 * 1024 * 1024 * 1024,
        vcpu_count: 6,
    });
    let output = probe.render_text();

    assert!(!probe.qemu_used);
    assert!(!probe.apple_vz_used);
    assert!(!probe.hvf_entered);
    assert_eq!(probe.format, "FDT");
    assert_eq!(probe.fdt_magic, FDT_MAGIC);
    assert_eq!(read_be_u32(&probe.fdt_blob, 0), Some(FDT_MAGIC));
    assert!(probe.fdt_magic_verified);
    assert_eq!(probe.memory_node_base_ipa, Some(WINDOWS_ARM_GUEST_RAM_IPA));
    assert!(probe.memory_node_at_guest_ram_base);
    assert_eq!(probe.requested_cpu_count, 6);
    assert_eq!(probe.cpu_count, 6);
    assert!(probe.cpu_count_verified);
    assert_eq!(probe.device_mmio_start_ipa, WINDOWS_ARM_DEVICE_MMIO_IPA);
    assert_eq!(
        probe.device_mmio_end_ipa,
        WINDOWS_ARM_DEVICE_MMIO_IPA + WINDOWS_ARM_DEVICE_MMIO_BYTES
    );
    assert_eq!(probe.mmio_nodes.len(), 4);
    assert!(probe
        .mmio_nodes
        .iter()
        .all(|node| node.inside_device_window));
    assert!(probe.mmio_nodes_inside_device_window);
    assert!(probe.mmio_nodes.iter().any(|node| node.label == "PL011"
        && node.base_ipa == Some(WINDOWS_ARM_PL011_MMIO_IPA)
        && node.bytes == Some(PL011_REGISTER_WINDOW_BYTES)));
    assert!(probe.mmio_nodes.iter().any(|node| node.label == "PL031"
        && node.base_ipa == Some(WINDOWS_ARM_PL031_MMIO_IPA)
        && node.bytes == Some(PL031_REGISTER_WINDOW_BYTES)));
    assert!(probe
        .mmio_nodes
        .iter()
        .any(|node| node.label == "VirtIO-MMIO installer ISO"
            && node.node_name == "virtio_mmio@10002000"
            && node.base_ipa == Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA)
            && node.bytes == Some(VIRTIO_MMIO_REGISTER_WINDOW_BYTES)));
    assert!(probe
        .mmio_nodes
        .iter()
        .any(|node| node.label == "VirtIO-MMIO target disk"
            && node.node_name == "virtio_mmio@10003000"
            && node.base_ipa == Some(WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA)
            && node.bytes == Some(VIRTIO_MMIO_REGISTER_WINDOW_BYTES)));
    assert_eq!(probe.root_interrupt_parent, Some(WINDOWS_ARM_GIC_PHANDLE));
    assert_eq!(probe.gic_phandle, Some(WINDOWS_ARM_GIC_PHANDLE));
    assert_eq!(
        probe.gic_distributor_base_ipa,
        Some(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA)
    );
    assert_eq!(
        probe.gic_distributor_bytes,
        Some(WINDOWS_ARM_GIC_DISTRIBUTOR_BYTES)
    );
    assert_eq!(
        probe.gic_redistributor_base_ipa,
        Some(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA)
    );
    assert_eq!(
        probe.gic_redistributor_bytes,
        Some(windows_arm_gic_redistributor_fdt_bytes(6))
    );
    assert!(probe.gic_nodes_inside_device_window);
    assert!(probe.arch_timer_node_present);
    assert_eq!(probe.arch_timer_interrupt_count, 4);
    assert_eq!(probe.interrupt_nodes.len(), 4);
    assert!(probe.interrupt_nodes_described);
    assert!(probe
        .interrupt_nodes
        .iter()
        .any(|node| node.label == "PL011"
            && node.interrupt_type == Some(GIC_SPI)
            && node.interrupt_number == Some(WINDOWS_ARM_PL011_SPI)
            && node.trigger == Some(IRQ_TYPE_LEVEL_HIGH)
            && node.described));
    assert!(probe
        .interrupt_nodes
        .iter()
        .any(|node| node.label == "PL031"
            && node.interrupt_type == Some(GIC_SPI)
            && node.interrupt_number == Some(WINDOWS_ARM_PL031_SPI)
            && node.trigger == Some(IRQ_TYPE_LEVEL_HIGH)
            && node.described));
    assert!(probe
        .interrupt_nodes
        .iter()
        .any(|node| node.label == "VirtIO-MMIO installer ISO"
            && node.interrupt_type == Some(GIC_SPI)
            && node.interrupt_number == Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI)
            && node.trigger == Some(IRQ_TYPE_LEVEL_HIGH)
            && node.described));
    assert!(probe
        .interrupt_nodes
        .iter()
        .any(|node| node.label == "VirtIO-MMIO target disk"
            && node.interrupt_type == Some(GIC_SPI)
            && node.interrupt_number == Some(WINDOWS_ARM_VIRTIO_TARGET_DISK_SPI)
            && node.trigger == Some(IRQ_TYPE_LEVEL_HIGH)
            && node.described));
    assert!(!probe.acpi_implemented);
    assert!(!probe.fw_cfg_used);
    assert_eq!(probe.gic_status, "described/not emulated");
    assert!(!probe.gic_emulated);
    assert!(probe.blockers.is_empty());

    assert!(output.contains("Windows 11 Arm HVF platform description probe"));
    assert!(output.contains("QEMU: not used"));
    assert!(output.contains("Apple VZ: not used"));
    assert!(output.contains("HVF: not entered"));
    assert!(output.contains("Format: FDT"));
    assert!(output.contains("FDT magic: 0xd00dfeed"));
    assert!(output.contains("Memory node base: 0x40000000"));
    assert!(output.contains("Memory node at 0x40000000: true"));
    assert!(output.contains("CPU count: 6"));
    assert!(output.contains("Device MMIO window: 0x10000000..0x20000000"));
    assert!(output.contains(
        "PL011/PL031/VirtIO-MMIO installer ISO/target disk nodes inside device window: true"
    ));
    assert!(output.contains("PL011 node inside device window: true"));
    assert!(output.contains("PL031 node inside device window: true"));
    assert!(output.contains("VirtIO-MMIO installer ISO node inside device window: true"));
    assert!(output.contains("VirtIO-MMIO target disk node inside device window: true"));
    assert!(output.contains("Root interrupt-parent: 0x1"));
    assert!(output.contains("GIC phandle: 0x1"));
    assert!(output.contains("GIC distributor base: 0x10010000"));
    assert!(output.contains("GIC distributor bytes: 0x10000"));
    assert!(output.contains("GIC redistributor base: 0x10020000"));
    assert!(output.contains("GIC redistributor bytes: 0xc0000"));
    assert!(output.contains("GIC nodes inside device window: true"));
    assert!(output.contains("ARM arch timer node present: true"));
    assert!(output.contains("ARM arch timer interrupt count: 4"));
    assert!(output.contains("Interrupt nodes described: true"));
    assert!(output.contains("PL011 interrupt type: 0x0"));
    assert!(output.contains("PL011 interrupt number: 0x0"));
    assert!(output.contains("PL011 interrupt trigger: 0x4"));
    assert!(output.contains("PL031 interrupt number: 0x1"));
    assert!(output.contains("VirtIO-MMIO installer ISO interrupt number: 0x2"));
    assert!(output.contains("VirtIO-MMIO target disk interrupt number: 0x3"));
    assert!(output.contains("ACPI: not implemented"));
    assert!(output.contains("fw_cfg: not used"));
    assert!(output.contains("GIC: described/not emulated"));
    assert!(output.contains("Blockers: none"));
    assert!(!output.contains("qemu-system"));
    assert!(!output.contains('%'));
}

#[test]
fn windows_11_arm_platform_description_probe_reports_zero_cpu_blocker() {
    let probe = probe_windows_11_arm_platform_description(WindowsArmPlatformDescriptionOptions {
        guest_ram_bytes: WINDOWS_ARM_PLATFORM_DESCRIPTION_DEFAULT_GUEST_RAM_BYTES,
        vcpu_count: 0,
    });
    let output = probe.render_text();

    assert_eq!(probe.requested_cpu_count, 0);
    assert_eq!(probe.cpu_count, 0);
    assert!(probe.cpu_count_verified);
    assert!(probe
        .blockers
        .iter()
        .any(|blocker| blocker.contains("FDT CPU count must be non-zero for Windows Arm")));
    assert!(output.contains("CPU count: 0"));
    assert!(output.contains("FDT CPU count must be non-zero for Windows Arm"));
    assert!(output.contains("QEMU: not used"));
    assert!(output.contains("Apple VZ: not used"));
    assert!(output.contains("HVF: not entered"));
    assert!(!output.contains("qemu-system"));
    assert!(!output.contains('%'));
}

#[test]
fn windows_11_arm_uefi_firmware_handoff_probe_creates_and_verifies_vars() {
    let stem = format!(
        "bridgevm-windows-arm-uefi-handoff-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
    let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
    let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
    std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
    std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
    let _ = std::fs::remove_file(&vars_path);

    let probe = probe_windows_11_arm_uefi_firmware_handoff(WindowsArmUefiFirmwareHandoffOptions {
        firmware_path: firmware_path.clone(),
        vars_template_path: Some(template_path.clone()),
        vars_path: Some(vars_path.clone()),
        create_vars: true,
    });
    let output = probe.render_text();
    let vars_bytes = std::fs::read(&vars_path).unwrap();
    let template_bytes = std::fs::read(&template_path).unwrap();
    let _ = std::fs::remove_file(&firmware_path);
    let _ = std::fs::remove_file(&template_path);
    let _ = std::fs::remove_file(&vars_path);

    assert_eq!(vars_bytes, template_bytes);
    assert_eq!(probe.firmware_path, firmware_path);
    assert_eq!(probe.firmware_bytes, Some(128 * 1024));
    assert!(probe.firmware_verified);
    assert_eq!(probe.firmware_slot_ipa, WINDOWS_ARM_UEFI_CODE_IPA);
    assert_eq!(probe.firmware_slot_bytes, WINDOWS_ARM_UEFI_SLOT_BYTES);
    assert_eq!(probe.vars_template_path, Some(template_path));
    assert_eq!(probe.vars_template_bytes, Some(64 * 1024));
    assert!(probe.vars_template_verified);
    assert_eq!(probe.vars_path, Some(vars_path));
    assert_eq!(probe.vars_bytes, Some(64 * 1024));
    assert_eq!(probe.vars_slot_ipa, WINDOWS_ARM_UEFI_VARS_IPA);
    assert_eq!(probe.vars_slot_bytes, WINDOWS_ARM_UEFI_SLOT_BYTES);
    assert!(probe.vars_created);
    assert!(probe.vars_reopened_for_verification);
    assert!(probe.vars_verified);
    assert_eq!(
        probe.planned_reset_vector_ipa,
        Some(WINDOWS_ARM_UEFI_CODE_IPA)
    );
    assert!(probe
        .firmware_volume
        .as_ref()
        .is_some_and(|volume| volume.checksum_verified));
    assert!(probe
        .vars_volume
        .as_ref()
        .is_some_and(|volume| volume.checksum_verified));
    assert!(probe.blockers.is_empty());
    assert!(output.contains("Windows 11 Arm HVF UEFI firmware handoff probe"));
    assert!(output.contains("QEMU: not used"));
    assert!(output.contains("Apple VZ: not used"));
    assert!(output.contains("HVF: not entered"));
    assert!(output.contains("AArch64 UEFI firmware and vars pflash handoff"));
    assert!(output.contains("Firmware verified: true"));
    assert!(output.contains("Firmware volume detected: true"));
    assert!(output.contains("Firmware volume checksum verified: true"));
    assert!(output.contains("Vars template verified: true"));
    assert!(output.contains("Vars created: true"));
    assert!(output.contains("Vars reopened for verification: true"));
    assert!(output.contains("Vars verified: true"));
    assert!(output.contains("Vars volume checksum verified: true"));
    assert!(output.contains("Planned reset vector IPA: 0x8000000"));
    assert!(output.contains("Blockers: none"));
    assert!(!output.contains("qemu-system"));
    assert!(!output.contains('%'));
}
