//! Split test module.

use super::super::*;
use crate::machine;

use super::helpers::*;

#[test]
fn checksum_makes_bytes_sum_to_zero() {
    let mut data = vec![1u8, 2, 3, 0];
    let n = data.len();
    data[n - 1] = checksum(&data);
    assert!(sums_to_zero(&data));
}

#[test]
fn every_table_carries_the_expected_signature() {
    let blobs = build_acpi(4);
    let tables = split_tables(&blobs.tables);
    let sigs: Vec<&str> = tables.iter().map(|(s, _)| s.as_str()).collect();
    // FADT's signature is "FACP" and MADT's is "APIC" by spec.
    for needed in [
        "XSDT", "DSDT", "FACP", "APIC", "PPTT", "GTDT", "MCFG", "SPCR", "DBG2",
    ] {
        assert!(sigs.contains(&needed), "missing table {needed} in {sigs:?}");
    }
    assert!(
        !sigs.contains(&"IORT"),
        "Apple GICM MSI-frame mode must not advertise ITS/IORT routing"
    );
}

#[test]
fn rsdp_has_signature_and_both_checksums_valid() {
    let blobs = build_acpi(1);
    let (rsdp, _) = replay_loader(&blobs);
    let rsdp = &rsdp;
    assert_eq!(&rsdp[..8], b"RSD PTR ");
    assert_eq!(rsdp.len(), 36);
    assert_eq!(rsdp[15], 2, "RSDP revision must be 2 (ACPI 2.0+)");
    // v1 checksum over the first 20 bytes.
    assert!(sums_to_zero(&rsdp[..20]), "RSDP v1 checksum invalid");
    // Extended checksum over all 36 bytes.
    assert!(sums_to_zero(rsdp), "RSDP extended checksum invalid");
}

#[test]
fn every_table_is_checksum_valid() {
    let blobs = build_acpi(8);
    let (_, tables) = replay_loader(&blobs);
    for (sig, table) in split_tables(&tables) {
        assert!(sums_to_zero(table), "table {sig} checksum invalid");
    }
}

#[test]
fn table_loader_has_qemu_shape_and_commands() {
    let blobs = build_acpi(2);
    assert_eq!(blobs.loader.len() % LOADER_ENTRY_LEN, 0);
    let commands: Vec<u32> = blobs
        .loader
        .chunks_exact(LOADER_ENTRY_LEN)
        .map(|entry| le32(entry, 0))
        .collect();
    assert_eq!(commands[0], LOADER_CMD_ALLOCATE);
    assert_eq!(commands[1], LOADER_CMD_ALLOCATE);
    assert_eq!(le_name(&blobs.loader, 4), ACPI_RSDP_FILE);
    assert_eq!(
        le_name(&blobs.loader, LOADER_ENTRY_LEN + 4),
        ACPI_TABLE_FILE
    );
    assert_eq!(
        commands
            .iter()
            .filter(|&&cmd| cmd == LOADER_CMD_ADD_POINTER)
            .count(),
        9
    );
    // Nine ACPI tables plus RSDP v1 and extended checksums.
    assert_eq!(
        commands
            .iter()
            .filter(|&&cmd| cmd == LOADER_CMD_ADD_CHECKSUM)
            .count(),
        11
    );
}

#[test]
fn rsdp_points_at_the_xsdt_and_xsdt_lists_every_table() {
    let blobs = build_acpi(2);
    // RSDP XsdtAddress is at offset 24 (after sig/cksum/oem/rev/rsdt/len).
    let xsdt_addr = le64(&blobs.rsdp, 24);
    let tables = split_tables(&blobs.tables);
    // XSDT is laid out first, so its address is the blob base (0).
    assert_eq!(xsdt_addr, 0);
    let xsdt = find(&tables, "XSDT");
    // XSDT entries are 8-byte pointers after the 36-byte header.
    let entry_count = (xsdt.len() - ACPI_HEADER_LEN) / 8;
    assert_eq!(
        entry_count, 7,
        "XSDT must list FADT/MADT/PPTT/GTDT/MCFG/SPCR/DBG2"
    );
    // Each listed pointer must land on a real table header in the blob.
    let valid_offsets: Vec<u64> = {
        let mut offs = Vec::new();
        let mut off = 0u64;
        for (_, t) in &tables {
            offs.push(off);
            off += t.len() as u64;
        }
        offs
    };
    for i in 0..entry_count {
        let ptr = le64(xsdt, ACPI_HEADER_LEN + i * 8);
        assert!(
            valid_offsets.contains(&ptr),
            "XSDT entry {i} = {ptr:#x} does not point at a table",
        );
    }
}

#[test]
fn fadt_is_hw_reduced_and_points_at_the_dsdt() {
    let blobs = build_acpi(1);
    let tables = split_tables(&blobs.tables);
    let fadt = find(&tables, "FACP");
    // Flags field sits at offset 112 in the FADT.
    let flags = le32(fadt, 112);
    assert_ne!(
        flags & FADT_FLAG_HW_REDUCED_ACPI,
        0,
        "HW_REDUCED_ACPI flag must be set on ARM",
    );
    assert_eq!(
        flags & (1 << 21),
        0,
        "LOW_POWER_S0_IDLE_CAPABLE must stay clear without platform idle support",
    );
    // ARM_BOOT_ARCH (u16) is at offset 129.
    let arm_boot = u16::from_le_bytes([fadt[129], fadt[130]]);
    assert_ne!(
        arm_boot & FADT_ARM_BOOT_PSCI_COMPLIANT,
        0,
        "FADT must declare PSCI compliance",
    );
    assert_ne!(
        arm_boot & FADT_ARM_BOOT_PSCI_USE_HVC,
        0,
        "FADT must declare PSCI via HVC",
    );
    // X_DSDT (u64) is at offset 140; it must point at the DSDT table.
    let x_dsdt = le64(fadt, 140);
    let dsdt_off = {
        let mut off = 0u64;
        let mut found = None;
        for (s, t) in &tables {
            if s == "DSDT" {
                found = Some(off);
                break;
            }
            off += t.len() as u64;
        }
        found.expect("DSDT present")
    };
    assert_eq!(x_dsdt, dsdt_off, "FADT X_DSDT must point at the DSDT");
}

#[test]
fn dsdt_names_qemu_like_uart_pci_and_power_devices() {
    let blobs = build_acpi(2);
    let tables = split_tables(&blobs.tables);
    let dsdt = find(&tables, "DSDT");
    assert!(
        dsdt.len() > ACPI_HEADER_LEN,
        "DSDT must carry AML, not just an empty definition block"
    );
    for needle in [
        b"_SB_".as_slice(),
        b"C000".as_slice(),
        b"C001".as_slice(),
        b"ACPI0007".as_slice(),
        b"COM0".as_slice(),
        b"ARMH0011".as_slice(),
        b"PCI0".as_slice(),
        b"_OSC".as_slice(),
        b"RES0".as_slice(),
        b"PWRB".as_slice(),
    ] {
        assert!(
            contains_bytes(dsdt, needle),
            "DSDT missing AML name/string {:?}",
            String::from_utf8_lossy(needle)
        );
    }
    assert!(
        contains_bytes(dsdt, &[AML_NAME_OP, b'_', b'U', b'I', b'D', AML_ONE_OP],),
        "DSDT must describe CPU ACPI0007 UIDs"
    );
    assert!(
        contains_bytes(
            dsdt,
            &resource_memory32_fixed(machine::UART.base, machine::UART.size)
        ),
        "DSDT must describe the PL011 MMIO window"
    );
    assert!(
        contains_bytes(
            dsdt,
            &resource_interrupt(machine::spi_to_intid(machine::SPI_UART))
        ),
        "DSDT must describe the PL011 GIC interrupt"
    );
    assert!(
        contains_bytes(dsdt, &resource_word_bus_number(0, 0x00FF)),
        "DSDT must describe PCI buses 00-ff"
    );
    assert!(
        contains_bytes(
            dsdt,
            &resource_dword_memory(machine::PCIE_MMIO_32.base, machine::PCIE_MMIO_32.size),
        ),
        "DSDT must describe the PCI 32-bit MMIO aperture"
    );
    assert!(
        contains_bytes(
            dsdt,
            &resource_qword_memory(
                machine::PCIE_MMIO_64_NON_PREFETCH.base,
                machine::PCIE_MMIO_64_NON_PREFETCH.size,
            )
        ),
        "DSDT must describe the non-prefetchable PCI 64-bit MMIO aperture"
    );
    assert!(
        contains_bytes(
            dsdt,
            &resource_qword_prefetchable_memory(
                machine::PCIE_MMIO_64_PREFETCH.base,
                machine::PCIE_MMIO_64_PREFETCH.size,
            )
        ),
        "DSDT must describe the prefetchable PCI 64-bit MMIO aperture"
    );
    assert!(
        contains_bytes(
            dsdt,
            &resource_dword_io(machine::PCIE_PIO.base, machine::PCIE_PIO.size),
        ),
        "DSDT must describe the translated PCI I/O aperture"
    );
    assert!(
        contains_bytes(
            dsdt,
            &resource_qword_memory(machine::PCIE_ECAM.base, machine::PCIE_ECAM.size),
        ),
        "DSDT must reserve the ECAM aperture through PNP0C02"
    );
}

#[test]
fn optional_tpm_tis_has_windows_hid_and_mmio_resource() {
    let blobs = build_acpi_with_devices(
        2,
        AcpiDeviceConfig {
            tpm_tis_present: true,
        },
    );
    let tables = split_tables(&blobs.tables);
    let dsdt = find(&tables, "DSDT");
    for needle in [
        b"TPM0".as_slice(),
        b"MSFT0101".as_slice(),
        b"TPM 2.0 Device".as_slice(),
    ] {
        assert!(contains_bytes(dsdt, needle), "DSDT missing TPM marker");
    }
    assert!(contains_bytes(
        dsdt,
        &resource_memory32_fixed(machine::TPM_TIS.base, machine::TPM_TIS.size)
    ));
    for marker in [
        b"_DSM".as_slice(),
        b"TPFN".as_slice(),
        b"PPRQ".as_slice(),
        b"PPRM".as_slice(),
        b"MOVV".as_slice(),
    ] {
        assert!(contains_bytes(dsdt, marker), "DSDT missing PPI AML marker");
    }
    assert!(contains_bytes(dsdt, &TPM_PPI_DSM_UUID));
    assert!(contains_bytes(dsdt, &TPM_RESET_ATTACK_DSM_UUID));
    assert!(contains_bytes(
        dsdt,
        &aml_dword((machine::TPM_PPI.base + 0x100) as u32)
    ));
}

#[test]
fn dsdt_pci_root_osc_matches_qemu_control_policy() {
    let blobs = build_acpi(1);
    let tables = split_tables(&blobs.tables);
    let dsdt = find(&tables, "DSDT");
    assert!(
        contains_bytes(dsdt, b"_OSC"),
        "PCI root bridge must expose _OSC"
    );
    assert!(
        contains_bytes(dsdt, &PCI_HOST_BRIDGE_OSC_UUID),
        "_OSC must use the PCI host bridge UUID"
    );
    for name in [b"CDW1".as_slice(), b"CDW2".as_slice(), b"CDW3".as_slice()] {
        assert!(
            contains_bytes(dsdt, name),
            "_OSC must create {} dword field",
            String::from_utf8_lossy(name)
        );
    }
    assert!(
        contains_bytes(dsdt, &[AML_AND_OP, AML_LOCAL0_OP, AML_BYTE_PREFIX, 0x1F]),
        "_OSC must mask OS-requested PCIe control to QEMU's supported feature set"
    );
    assert!(
        contains_bytes(
            dsdt,
            &[AML_OR_OP, b'C', b'D', b'W', b'1', AML_BYTE_PREFIX, 0x10]
        ),
        "_OSC must set the capabilities-masked status bit when denying control bits"
    );
    assert!(
        contains_bytes(dsdt, &[AML_RETURN_OP, AML_ARG0_OP + 3]),
        "_OSC must return Arg3"
    );
}

#[test]
fn madt_has_one_gicc_per_cpu_plus_gicd_and_gicr() {
    for cpu_count in [1u64, 2, 8, 16, 17] {
        let blobs = build_acpi(cpu_count);
        let tables = split_tables(&blobs.tables);
        let madt = find(&tables, "APIC");
        assert_eq!(madt[8], 4, "MADT revision must match QEMU virt");
        // Walk the interrupt-controller structures after the 8-byte MADT body.
        let mut off = ACPI_HEADER_LEN + 8;
        let mut gicc = 0u64;
        let mut gicd = 0u64;
        let mut gicr = 0u64;
        let mut gic_msi_frame = 0u64;
        while off < madt.len() {
            let typ = madt[off];
            let len = madt[off + 1] as usize;
            assert!(len > 0, "zero-length MADT entry");
            match typ {
                0x0B => gicc += 1,
                0x0C => gicd += 1,
                0x0D => {
                    gic_msi_frame += 1;
                    assert_eq!(len, 24, "Generic MSI Frame MADT entry length");
                    assert_eq!(le32(madt, off + 4), 0, "MSI Frame ID");
                    assert_eq!(
                        le64(madt, off + 8),
                        machine::GIC_MSI_FRAME.base,
                        "MSI frame base"
                    );
                    assert_eq!(le32(madt, off + 16), 1, "override SPI values flag");
                    assert_eq!(
                        u16::from_le_bytes([madt[off + 20], madt[off + 21]]),
                        machine::GIC_MSI_INTID_COUNT as u16,
                        "MSI frame SPI count"
                    );
                    assert_eq!(
                        u16::from_le_bytes([madt[off + 22], madt[off + 23]]),
                        machine::GIC_MSI_INTID_BASE as u16,
                        "MSI frame SPI base"
                    );
                }
                0x0E => gicr += 1,
                0x0F => panic!("Apple GICM mode must not advertise a GIC ITS entry"),
                _ => {}
            }
            off += len;
        }
        assert_eq!(off, madt.len(), "MADT entries must tile the table exactly");
        assert_eq!(gicc, cpu_count, "one GICC per CPU");
        assert_eq!(gicd, 1, "exactly one GICD");
        assert_eq!(gicr, 1, "exactly one GICR discovery range");
        assert_eq!(gic_msi_frame, 1, "exactly one Generic MSI Frame");
    }
}

#[test]
fn madt_gicc_redistributor_base_uses_machine_constants() {
    let blobs = build_acpi(3);
    let tables = split_tables(&blobs.tables);
    let madt = find(&tables, "APIC");
    // QEMU emits the GICD after the MADT body; the first GICC follows it.
    let gicc0 = ACPI_HEADER_LEN + 8 + 24;
    assert_eq!(
        le32(madt, gicc0 + 20),
        ppi_to_gsiv(machine::PPI_PMU),
        "GICC must advertise QEMU-like PMU PPI GSIV",
    );
    let gicr_base = le64(madt, gicc0 + 60);
    assert_eq!(gicr_base, machine::GIC_REDIST.base);
    // Second CPU's redistributor is one stride higher.
    let gicc1 = gicc0 + madt[gicc0 + 1] as usize;
    let gicr_base1 = le64(madt, gicc1 + 60);
    assert_eq!(
        gicr_base1,
        machine::GIC_REDIST.base + machine::GICV3_REDIST_STRIDE,
    );
}

#[test]
fn madt_gicd_uses_machine_dist_base() {
    let blobs = build_acpi(1);
    let tables = split_tables(&blobs.tables);
    let madt = find(&tables, "APIC");
    // The GICD follows the 8-byte MADT body, matching QEMU virt.
    let gicd = ACPI_HEADER_LEN + 8;
    assert_eq!(madt[gicd], 0x0C, "expected GICD at computed offset");
    let dist_base = le64(madt, gicd + 8);
    assert_eq!(dist_base, machine::GIC_DIST.base);
    assert_eq!(madt[gicd + 20], 3, "GIC version must be 3");
}

#[test]
fn pptt_has_qemu_like_root_socket_and_cpu_leaf_nodes() {
    let cpu_count = 3u64;
    let blobs = build_acpi(cpu_count);
    let tables = split_tables(&blobs.tables);
    let pptt = find(&tables, "PPTT");
    assert_eq!(pptt[8], 2, "PPTT revision must match QEMU");

    let mut nodes = Vec::new();
    let mut off = ACPI_HEADER_LEN;
    while off < pptt.len() {
        let typ = pptt[off];
        let len = pptt[off + 1] as usize;
        assert_eq!(typ, PPTT_NODE_PROCESSOR, "only processor nodes expected");
        assert_eq!(len, 20, "PPTT processor nodes have no private resources");
        nodes.push((
            off as u32,
            le32(pptt, off + 4),
            le32(pptt, off + 8),
            le32(pptt, off + 12),
            le32(pptt, off + 16),
        ));
        off += len;
    }

    assert_eq!(off, pptt.len(), "PPTT nodes must tile the table exactly");
    assert_eq!(nodes.len(), 2 + cpu_count as usize);

    let root = nodes[0];
    assert_eq!(root.0, ACPI_HEADER_LEN as u32);
    assert_eq!(
        root.1,
        PPTT_PROCESSOR_PHYSICAL_PACKAGE | PPTT_PROCESSOR_IDENTICAL
    );
    assert_eq!(root.2, 0, "root node has no parent");
    assert_eq!(root.3, 0, "root package ID");
    assert_eq!(root.4, 0, "root has no private resources");

    let socket = nodes[1];
    assert_eq!(
        socket.1,
        PPTT_PROCESSOR_PHYSICAL_PACKAGE | PPTT_PROCESSOR_IDENTICAL
    );
    assert_eq!(socket.2, root.0, "socket parent must be the root node");
    assert_eq!(socket.3, 0, "single socket ID");
    assert_eq!(socket.4, 0, "socket has no private resources");

    for (idx, node) in nodes[2..].iter().enumerate() {
        assert_eq!(node.1, PPTT_PROCESSOR_ACPI_ID_VALID | PPTT_PROCESSOR_LEAF);
        assert_eq!(node.2, socket.0, "CPU leaf parent must be the socket");
        assert_eq!(node.3, idx as u32, "CPU leaf ID matches ACPI UID");
        assert_eq!(node.4, 0, "CPU leaf has no private resources");
    }
}

#[test]
fn mcfg_base_matches_machine_pcie_ecam() {
    let blobs = build_acpi(4);
    let tables = split_tables(&blobs.tables);
    let mcfg = find(&tables, "MCFG");
    // header(36) + reserved(8) -> first allocation entry.
    let base = le64(mcfg, ACPI_HEADER_LEN + 8);
    assert_eq!(base, machine::PCIE_ECAM.base);
    // Buses 0..=255.
    let start_bus = mcfg[ACPI_HEADER_LEN + 8 + 10];
    let end_bus = mcfg[ACPI_HEADER_LEN + 8 + 11];
    assert_eq!(start_bus, 0);
    assert_eq!(end_bus, 0xFF);
}

#[test]
fn gtdt_virtual_timer_gsiv_is_ppi_timer_virt() {
    let blobs = build_acpi(1);
    let tables = split_tables(&blobs.tables);
    let gtdt = find(&tables, "GTDT");
    // Layout: header(36) CntControlBase(8) reserved(4)
    //   secure GSIV(4) secure flags(4)
    //   nonsec GSIV(4) nonsec flags(4)
    //   virtual GSIV(4) ...
    let virt_gsiv = le32(gtdt, ACPI_HEADER_LEN + 8 + 4 + 8 + 8);
    assert_eq!(virt_gsiv, machine::PPI_TIMER_VIRT + 16);
    // Sanity: the secure timer GSIV is the secure PPI + 16.
    let secure_gsiv = le32(gtdt, ACPI_HEADER_LEN + 8 + 4);
    assert_eq!(secure_gsiv, machine::PPI_TIMER_SECURE + 16);
}

#[test]
fn spcr_targets_the_pl011_console() {
    let blobs = build_acpi(1);
    let tables = split_tables(&blobs.tables);
    let spcr = find(&tables, "SPCR");
    assert_eq!(spcr[ACPI_HEADER_LEN], 0x03, "interface type = ARM PL011");
    assert_eq!(
        spcr[ACPI_HEADER_LEN + 4 + 3],
        3,
        "SPCR GAS access size must be dword"
    );
    // GAS base address is at header + interface_type(1) + reserved(3) + 4.
    let gas_addr = le64(spcr, ACPI_HEADER_LEN + 4 + 4);
    assert_eq!(gas_addr, machine::UART.base);
    // GSIV is at header + 4 + 12(GAS) + interrupt_type(1) + irq(1).
    let gsiv = le32(spcr, ACPI_HEADER_LEN + 4 + 12 + 2);
    assert_eq!(gsiv, machine::spi_to_intid(machine::SPI_UART));
}

#[test]
fn dbg2_describes_the_pl011_debug_port() {
    let blobs = build_acpi(1);
    let tables = split_tables(&blobs.tables);
    let dbg2 = find(&tables, "DBG2");
    assert_eq!(dbg2[8], 0, "DBG2 revision must match QEMU virt");

    let device_info_offset = le32(dbg2, ACPI_HEADER_LEN) as usize;
    assert_eq!(device_info_offset, 44);
    assert_eq!(le32(dbg2, ACPI_HEADER_LEN + 4), 1);

    let dev = device_info_offset;
    assert_eq!(dbg2[dev], 0, "Debug Device Information revision");
    assert_eq!(le16(dbg2, dev + 1), 43, "Debug Device Information length");
    assert_eq!(dbg2[dev + 3], 1, "one GAS register");
    assert_eq!(le16(dbg2, dev + 4), 5, "COM0 namespace length");
    assert_eq!(le16(dbg2, dev + 6), 38, "namespace string offset");
    assert_eq!(le16(dbg2, dev + 12), 0x8000, "Port Type = Serial");
    assert_eq!(le16(dbg2, dev + 14), 0x0003, "Port Subtype = ARM PL011");

    let gas_off = dev + le16(dbg2, dev + 18) as usize;
    assert_eq!(dbg2[gas_off], 0, "DBG2 GAS must be system memory");
    assert_eq!(dbg2[gas_off + 1], 32, "DBG2 GAS register width");
    assert_eq!(dbg2[gas_off + 3], 3, "DBG2 GAS access size must be dword");
    assert_eq!(le64(dbg2, gas_off + 4), machine::UART.base);

    let size_off = dev + le16(dbg2, dev + 20) as usize;
    assert_eq!(le32(dbg2, size_off), machine::UART.size as u32);

    let namespace_off = dev + le16(dbg2, dev + 6) as usize;
    assert_eq!(&dbg2[namespace_off..namespace_off + 5], b"COM0\0");
}

#[test]
fn build_acpi_is_deterministic() {
    assert_eq!(build_acpi(4), build_acpi(4));
}

#[test]
#[should_panic(expected = "exceeds GICv3 redistributor window")]
fn build_acpi_rejects_too_many_cpus() {
    build_acpi(machine::MAX_CPUS + 1);
}
