//! Split out of windows_arm.rs by responsibility.

use super::*;
use crate::*;

pub fn probe_windows_11_arm_platform_description(
    options: WindowsArmPlatformDescriptionOptions,
) -> WindowsArmPlatformDescriptionProbe {
    let fdt_blob = build_windows_arm_platform_fdt_blob(&options);
    let summary = inspect_windows_arm_platform_fdt_blob(&fdt_blob);
    let device_mmio_end_ipa =
        WINDOWS_ARM_DEVICE_MMIO_IPA.saturating_add(WINDOWS_ARM_DEVICE_MMIO_BYTES);
    let mmio_nodes = vec![
        WindowsArmFdtMmioNodeCheck {
            label: "PL011",
            node_name: "serial@10000000",
            base_ipa: summary.pl011.map(|range| range.base_ipa),
            bytes: summary.pl011.map(|range| range.bytes),
            inside_device_window: summary.pl011.is_some_and(fdt_range_inside_device_window),
        },
        WindowsArmFdtMmioNodeCheck {
            label: "PL031",
            node_name: "rtc@10001000",
            base_ipa: summary.pl031.map(|range| range.base_ipa),
            bytes: summary.pl031.map(|range| range.bytes),
            inside_device_window: summary.pl031.is_some_and(fdt_range_inside_device_window),
        },
        WindowsArmFdtMmioNodeCheck {
            label: "VirtIO-MMIO installer ISO",
            node_name: "virtio_mmio@10002000",
            base_ipa: summary.virtio_installer_iso.map(|range| range.base_ipa),
            bytes: summary.virtio_installer_iso.map(|range| range.bytes),
            inside_device_window: summary
                .virtio_installer_iso
                .is_some_and(fdt_range_inside_device_window),
        },
        WindowsArmFdtMmioNodeCheck {
            label: "VirtIO-MMIO target disk",
            node_name: "virtio_mmio@10003000",
            base_ipa: summary.virtio_target_disk.map(|range| range.base_ipa),
            bytes: summary.virtio_target_disk.map(|range| range.bytes),
            inside_device_window: summary
                .virtio_target_disk
                .is_some_and(fdt_range_inside_device_window),
        },
    ];
    let mmio_nodes_inside_device_window = mmio_nodes.iter().all(|node| node.inside_device_window);
    let gic_nodes_inside_device_window = summary
        .gic_distributor
        .is_some_and(fdt_range_inside_device_window)
        && summary
            .gic_redistributor
            .is_some_and(fdt_range_inside_device_window);
    let arch_timer_node_present = !summary.arch_timer_interrupts.is_empty();
    let arch_timer_interrupt_count = summary.arch_timer_interrupts.len();
    let interrupt_nodes = vec![
        WindowsArmFdtInterruptCheck {
            label: "PL011",
            node_name: "serial@10000000",
            interrupt_type: summary
                .pl011_interrupt
                .map(|interrupt| interrupt.interrupt_type),
            interrupt_number: summary
                .pl011_interrupt
                .map(|interrupt| interrupt.interrupt_number),
            trigger: summary.pl011_interrupt.map(|interrupt| interrupt.trigger),
            described: summary.pl011_interrupt.is_some(),
        },
        WindowsArmFdtInterruptCheck {
            label: "PL031",
            node_name: "rtc@10001000",
            interrupt_type: summary
                .pl031_interrupt
                .map(|interrupt| interrupt.interrupt_type),
            interrupt_number: summary
                .pl031_interrupt
                .map(|interrupt| interrupt.interrupt_number),
            trigger: summary.pl031_interrupt.map(|interrupt| interrupt.trigger),
            described: summary.pl031_interrupt.is_some(),
        },
        WindowsArmFdtInterruptCheck {
            label: "VirtIO-MMIO installer ISO",
            node_name: "virtio_mmio@10002000",
            interrupt_type: summary
                .virtio_installer_iso_interrupt
                .map(|interrupt| interrupt.interrupt_type),
            interrupt_number: summary
                .virtio_installer_iso_interrupt
                .map(|interrupt| interrupt.interrupt_number),
            trigger: summary
                .virtio_installer_iso_interrupt
                .map(|interrupt| interrupt.trigger),
            described: summary.virtio_installer_iso_interrupt.is_some(),
        },
        WindowsArmFdtInterruptCheck {
            label: "VirtIO-MMIO target disk",
            node_name: "virtio_mmio@10003000",
            interrupt_type: summary
                .virtio_target_disk_interrupt
                .map(|interrupt| interrupt.interrupt_type),
            interrupt_number: summary
                .virtio_target_disk_interrupt
                .map(|interrupt| interrupt.interrupt_number),
            trigger: summary
                .virtio_target_disk_interrupt
                .map(|interrupt| interrupt.trigger),
            described: summary.virtio_target_disk_interrupt.is_some(),
        },
    ];
    let interrupt_nodes_described = interrupt_nodes.iter().all(|node| node.described);
    let memory_node_at_guest_ram_base =
        summary.memory_node_base_ipa == Some(WINDOWS_ARM_GUEST_RAM_IPA);
    let cpu_count_verified = summary.cpu_count == options.vcpu_count;
    let mut blockers = summary.blockers;

    if options.guest_ram_bytes == 0 {
        blockers.push("guest RAM FDT reg size must be non-zero".to_string());
    }
    if options.vcpu_count == 0 {
        blockers.push("FDT CPU count must be non-zero for Windows Arm".to_string());
    }
    if summary.fdt_magic != FDT_MAGIC {
        blockers.push("FDT header magic did not match 0xd00dfeed".to_string());
    }
    if !memory_node_at_guest_ram_base {
        blockers.push("FDT memory node is not rooted at the Windows Arm guest RAM IPA".to_string());
    }
    if !cpu_count_verified {
        blockers.push("FDT CPU node count does not match requested vCPU count".to_string());
    }
    if !mmio_nodes_inside_device_window {
        blockers.push(
            "FDT PL011/PL031/VirtIO-MMIO installer ISO/target disk nodes are not fully inside the Windows device window"
                .to_string(),
        );
    }
    if summary.root_interrupt_parent != Some(WINDOWS_ARM_GIC_PHANDLE) {
        blockers.push("FDT root interrupt-parent does not point at the GIC phandle".to_string());
    }
    if summary.gic_phandle != Some(WINDOWS_ARM_GIC_PHANDLE) || !summary.gic_interrupt_controller {
        blockers.push("FDT GICv3 interrupt-controller node is incomplete".to_string());
    }
    if !gic_nodes_inside_device_window {
        blockers.push(
            "FDT GIC distributor/redistributor nodes are not fully inside the Windows device window"
                .to_string(),
        );
    }
    if arch_timer_interrupt_count != 4 {
        blockers.push("FDT ARM arch timer must describe four timer interrupts".to_string());
    }
    if !interrupt_nodes_described {
        blockers
            .push("FDT PL011/PL031/VirtIO-MMIO interrupt properties are incomplete".to_string());
    }

    WindowsArmPlatformDescriptionProbe {
        qemu_used: false,
        apple_vz_used: false,
        hvf_entered: false,
        format: "FDT",
        fdt_blob_bytes: fdt_blob.len(),
        fdt_blob,
        fdt_magic: summary.fdt_magic,
        fdt_magic_verified: summary.fdt_magic == FDT_MAGIC,
        memory_node_base_ipa: summary.memory_node_base_ipa,
        memory_node_at_guest_ram_base,
        requested_cpu_count: options.vcpu_count,
        cpu_count: summary.cpu_count,
        cpu_count_verified,
        device_mmio_start_ipa: WINDOWS_ARM_DEVICE_MMIO_IPA,
        device_mmio_end_ipa,
        mmio_nodes,
        mmio_nodes_inside_device_window,
        root_interrupt_parent: summary.root_interrupt_parent,
        gic_phandle: summary.gic_phandle,
        gic_distributor_base_ipa: summary.gic_distributor.map(|range| range.base_ipa),
        gic_distributor_bytes: summary.gic_distributor.map(|range| range.bytes),
        gic_redistributor_base_ipa: summary.gic_redistributor.map(|range| range.base_ipa),
        gic_redistributor_bytes: summary.gic_redistributor.map(|range| range.bytes),
        gic_nodes_inside_device_window,
        arch_timer_node_present,
        arch_timer_interrupt_count,
        interrupt_nodes,
        interrupt_nodes_described,
        acpi_implemented: false,
        fw_cfg_used: false,
        gic_status: "described/not emulated",
        gic_emulated: false,
        blockers,
    }
}

pub(crate) fn windows_arm_firmware_block_devices(
    installer_iso_path: Option<PathBuf>,
    writable_target_disk_path: Option<PathBuf>,
) -> Vec<WindowsArmVirtioBlockDeviceMetadata> {
    let installer_capacity_sectors =
        windows_arm_block_capacity_sectors(installer_iso_path.as_ref());
    let target_capacity_sectors =
        windows_arm_block_capacity_sectors(writable_target_disk_path.as_ref());
    vec![
        WindowsArmVirtioBlockDeviceMetadata {
            role: "installer-iso",
            label: "VirtIO-MMIO installer ISO",
            node_name: "virtio_mmio@10002000",
            base_ipa: WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA,
            bytes: VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
            read_only: true,
            backing_kind: "host-iso-readonly",
            backing_path: installer_iso_path,
            device_features: VIRTIO_BLK_F_RO,
            capacity_sectors: installer_capacity_sectors,
        },
        WindowsArmVirtioBlockDeviceMetadata {
            role: "target-disk",
            label: "VirtIO-MMIO target disk",
            node_name: "virtio_mmio@10003000",
            base_ipa: WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA,
            bytes: VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
            read_only: false,
            backing_kind: "host-file-writable",
            backing_path: writable_target_disk_path,
            device_features: VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE,
            capacity_sectors: target_capacity_sectors,
        },
    ]
}

pub(crate) fn windows_arm_block_capacity_sectors(path: Option<&PathBuf>) -> u64 {
    path.and_then(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.len() / VIRTIO_BLOCK_SECTOR_BYTES)
        .unwrap_or(VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS)
}
