//! ACPI table generator for the BridgeVM HVF "QEMU virt contract" path (Path A).
//!
//! Windows 11 ARM (and an ACPI-only Linux boot) refuses to come up without ACPI
//! tables: the firmware hands the guest an RSDP that chains to the XSDT, FADT,
//! MADT (GIC topology + the Apple `hv_gic` MSI frame), PPTT (CPU topology), GTDT
//! (architected timer), MCFG (PCIe ECAM), SPCR (serial console) and DBG2
//! (debug serial port metadata).
//! Stock ArmVirtQemu firmware does not synthesise these itself
//! on this platform — it installs whatever the host exposes through `fw_cfg`
//! under `etc/acpi/rsdp` (the RSDP), `etc/acpi/tables` (the concatenated
//! tables), and `etc/table-loader` (QEMU linker commands for relocation and
//! checksums). This module builds those blobs from the single source of truth in
//! [`crate::machine`], so every address and interrupt number matches the DTB the
//! same firmware parses (see [`crate::dtb`]).
//!
//! It is a self-contained, host-only byte serializer in the style of
//! [`crate::dtb`] / [`crate::fwcfg`]: no Hypervisor.framework calls, fully
//! unit-tested. ACPI integers are little-endian (unlike the big-endian DTB).
//!
//! References: ACPI 6.5 (RSDP §5.2.5, XSDT §5.2.8, FADT §5.2.9, MADT §5.2.12,
//! PPTT §5.2.29, GTDT §5.2.25, MCFG (PCI Firmware Spec 3.3), SPCR (Microsoft
//! Serial Port Console Redirection Table), DBG2 (Microsoft Debug Port Table 2)),
//! and the tables QEMU's
//! `hw/arm/virt-acpi-build.c` emits for the `virt` machine.

use crate::machine;

/// Length of an ACPI standard description-header (`signature` .. `creator_revision`).
const ACPI_HEADER_LEN: usize = 36;

/// OEM identity stamped into every table header (6 + 8 + 4 bytes).
const OEM_ID: &[u8; 6] = b"BRDGVM";
const OEM_TABLE_ID: &[u8; 8] = b"BVMVIRT ";
const OEM_REVISION: u32 = 1;
const CREATOR_ID: &[u8; 4] = b"BVM ";
const CREATOR_REVISION: u32 = 1;

/// QEMU fw_cfg file carrying the concatenated ACPI tables.
pub const ACPI_TABLE_FILE: &str = "etc/acpi/tables";
/// QEMU fw_cfg file carrying the RSDP.
pub const ACPI_RSDP_FILE: &str = "etc/acpi/rsdp";
/// QEMU fw_cfg file carrying loader/linker commands.
pub const ACPI_LOADER_FILE: &str = "etc/table-loader";

/// The three blobs the firmware fetches from `fw_cfg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpiBlobs {
    /// `etc/acpi/rsdp` — the Root System Description Pointer (36 bytes, v2).
    ///
    /// Checksum bytes are zero here; the firmware computes final checksums after
    /// applying `loader` relocations, matching QEMU's `bios-linker-loader`.
    pub rsdp: Vec<u8>,
    /// `etc/acpi/tables` — XSDT, FADT, DSDT, MADT, PPTT, GTDT, MCFG, SPCR and
    /// DBG2, concatenated in the order their physical addresses are laid out.
    ///
    /// Checksum bytes are zero here; the firmware computes final checksums after
    /// applying `loader` relocations, matching QEMU's `bios-linker-loader`.
    pub tables: Vec<u8>,
    /// `etc/table-loader` — QEMU loader commands that allocate the two files,
    /// relocate all table-internal pointers, and compute final ACPI checksums.
    pub loader: Vec<u8>,
}

/// One-byte ACPI checksum: the value that makes the sum of every byte in
/// `bytes` (including the checksum byte itself) wrap to zero mod 256.
fn checksum(bytes: &[u8]) -> u8 {
    let sum = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    sum.wrapping_neg()
}

/// A description table under construction. Reserves the 36-byte header up front
/// and exposes little-endian append helpers; [`Self::finish`] back-patches the
/// length and checksum so the finished blob sums to zero.
struct Table {
    bytes: Vec<u8>,
}

impl Table {
    /// Begin a table with the given 4-byte signature and revision, reserving a
    /// zeroed standard header.
    fn new(signature: &[u8; 4], revision: u8) -> Self {
        let mut bytes = Vec::with_capacity(ACPI_HEADER_LEN);
        bytes.extend_from_slice(signature);
        bytes.extend_from_slice(&0u32.to_le_bytes()); // length (patched in finish)
        bytes.push(revision);
        bytes.push(0); // checksum (patched in finish)
        bytes.extend_from_slice(OEM_ID);
        bytes.extend_from_slice(OEM_TABLE_ID);
        bytes.extend_from_slice(&OEM_REVISION.to_le_bytes());
        bytes.extend_from_slice(CREATOR_ID);
        bytes.extend_from_slice(&CREATOR_REVISION.to_le_bytes());
        debug_assert_eq!(bytes.len(), ACPI_HEADER_LEN);
        Self { bytes }
    }

    fn u8(&mut self, v: u8) {
        self.bytes.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }
    /// Append `n` zero bytes (reserved fields).
    fn pad(&mut self, n: usize) {
        self.bytes.extend(std::iter::repeat(0u8).take(n));
    }

    /// A 12-byte ACPI Generic Address Structure (GAS) with an explicit ACPI
    /// access-size encoding (1=byte, 2=word,
    /// 3=dword, 4=qword). SPCR consumers warn if this is left undefined.
    fn gas_memory_with_access_size(&mut self, address: u64, bit_width: u8, access_size: u8) {
        self.u8(0x00); // AddressSpaceId = SystemMemory
        self.u8(bit_width);
        self.u8(0x00); // BitOffset
        self.u8(access_size);
        self.u64(address);
    }

    /// A null Generic Address Structure (all fields zero) — used where the spec
    /// allows "not present".
    fn gas_null(&mut self) {
        self.pad(12);
    }

    /// Back-patch length + checksum and return the finished bytes.
    fn finish(mut self) -> Vec<u8> {
        let len = self.bytes.len() as u32;
        self.bytes[4..8].copy_from_slice(&len.to_le_bytes());
        // Header checksum byte is at offset 9; compute over the whole table.
        self.bytes[9] = 0;
        self.bytes[9] = checksum(&self.bytes);
        self.bytes
    }
}

// ---- FADT (Fixed ACPI Description Table) flags ------------------------------

/// `HW_REDUCED_ACPI` — there is no ACPI register hardware; events and power
/// control go through alternative mechanisms (mandatory on ARM).
const FADT_FLAG_HW_REDUCED_ACPI: u32 = 1 << 20;

/// ARM boot architecture flags (FADT offset 129).
const FADT_ARM_BOOT_PSCI_COMPLIANT: u16 = 1 << 0;
/// PSCI is invoked via `HVC` rather than `SMC`.
const FADT_ARM_BOOT_PSCI_USE_HVC: u16 = 1 << 1;

// ---- Builder ----------------------------------------------------------------

/// Build the `etc/acpi/rsdp` and `etc/acpi/tables` blobs for a `cpu_count`-CPU
/// guest. Panics if `cpu_count` exceeds what the GICv3 redistributor window can
/// host (mirrors [`crate::dtb::build_virt_fdt`]).
pub fn build_acpi(cpu_count: u64) -> AcpiBlobs {
    assert!(cpu_count >= 1, "ACPI requires at least one CPU");
    assert!(
        machine::redist_fits(cpu_count),
        "cpu_count {cpu_count} exceeds GICv3 redistributor window",
    );

    // Lay the tables out back-to-back; the XSDT must point at each one's
    // guest-physical address, so we assign addresses as we concatenate. The base
    // is arbitrary here (the firmware relocates the blob and rewrites pointers
    // via etc/table-loader), but internal pointers must be self-consistent.
    const TABLES_BASE: u64 = 0;

    let dsdt = build_dsdt(cpu_count);
    let madt = build_madt(cpu_count);
    let pptt = build_pptt(cpu_count);
    let gtdt = build_gtdt();
    let mcfg = build_mcfg();
    let spcr = build_spcr();
    let dbg2 = build_dbg2();

    // The XSDT references FADT/MADT/PPTT/GTDT/MCFG/SPCR/DBG2. The FADT
    // references the DSDT. Compute offsets in concatenation order: XSDT first,
    // then the rest. (Order within the blob is a free choice; we keep XSDT first
    // so its address is easy to reason about, then DSDT, then the XSDT-listed
    // tables.)
    let xsdt_len = xsdt_len_for(7);
    let off_xsdt = 0u64;
    let off_dsdt = off_xsdt + xsdt_len;
    let off_fadt = off_dsdt + dsdt.len() as u64;
    let off_madt = off_fadt + fadt_len() as u64;
    let off_pptt = off_madt + madt.len() as u64;
    let off_gtdt = off_pptt + pptt.len() as u64;
    let off_mcfg = off_gtdt + gtdt.len() as u64;
    let off_spcr = off_mcfg + mcfg.len() as u64;
    let off_dbg2 = off_spcr + spcr.len() as u64;

    let fadt = build_fadt(TABLES_BASE + off_dsdt);
    debug_assert_eq!(fadt.len() as u64, fadt_len());

    let xsdt = build_xsdt(&[
        TABLES_BASE + off_fadt,
        TABLES_BASE + off_madt,
        TABLES_BASE + off_pptt,
        TABLES_BASE + off_gtdt,
        TABLES_BASE + off_mcfg,
        TABLES_BASE + off_spcr,
        TABLES_BASE + off_dbg2,
    ]);
    debug_assert_eq!(xsdt.len() as u64, xsdt_len);

    let table_spans = [
        TableSpan::new(off_xsdt, xsdt.len() as u64),
        TableSpan::new(off_dsdt, dsdt.len() as u64),
        TableSpan::new(off_fadt, fadt.len() as u64),
        TableSpan::new(off_madt, madt.len() as u64),
        TableSpan::new(off_pptt, pptt.len() as u64),
        TableSpan::new(off_gtdt, gtdt.len() as u64),
        TableSpan::new(off_mcfg, mcfg.len() as u64),
        TableSpan::new(off_spcr, spcr.len() as u64),
        TableSpan::new(off_dbg2, dbg2.len() as u64),
    ];

    let mut tables = Vec::new();
    tables.extend_from_slice(&xsdt);
    tables.extend_from_slice(&dsdt);
    tables.extend_from_slice(&fadt);
    tables.extend_from_slice(&madt);
    tables.extend_from_slice(&pptt);
    tables.extend_from_slice(&gtdt);
    tables.extend_from_slice(&mcfg);
    tables.extend_from_slice(&spcr);
    tables.extend_from_slice(&dbg2);

    let mut rsdp = build_rsdp(TABLES_BASE + off_xsdt);
    let loader = build_table_loader(
        &mut rsdp,
        &mut tables,
        LoaderLayout {
            xsdt: off_xsdt,
            fadt: off_fadt,
            table_spans: &table_spans,
            xsdt_entries: &[
                off_fadt, off_madt, off_pptt, off_gtdt, off_mcfg, off_spcr, off_dbg2,
            ],
        },
    );

    AcpiBlobs {
        rsdp,
        tables,
        loader,
    }
}

#[derive(Debug, Clone, Copy)]
struct TableSpan {
    start: u32,
    len: u32,
}

impl TableSpan {
    fn new(start: u64, len: u64) -> Self {
        Self {
            start: u32::try_from(start).expect("ACPI table offset exceeds 4 GiB"),
            len: u32::try_from(len).expect("ACPI table length exceeds 4 GiB"),
        }
    }
}

struct LoaderLayout<'a> {
    xsdt: u64,
    fadt: u64,
    table_spans: &'a [TableSpan],
    xsdt_entries: &'a [u64],
}

const LOADER_ENTRY_LEN: usize = 128;
const LOADER_PAYLOAD_LEN: usize = 124;
const LOADER_FILE_NAME_LEN: usize = 56;

const LOADER_CMD_ALLOCATE: u32 = 1;
const LOADER_CMD_ADD_POINTER: u32 = 2;
const LOADER_CMD_ADD_CHECKSUM: u32 = 3;

const LOADER_ZONE_HIGH: u8 = 1;
const LOADER_ZONE_FSEG: u8 = 2;

const TABLE_ALLOC_ALIGN: u32 = 64;
const RSDP_ALLOC_ALIGN: u32 = 16;
const ACPI_CHECKSUM_OFFSET: u32 = 9;
const RSDP_V1_CHECKSUM_OFFSET: u32 = 8;
const RSDP_EXT_CHECKSUM_OFFSET: u32 = 32;
const RSDP_XSDT_OFFSET: u32 = 24;
const FADT_X_DSDT_OFFSET: u32 = 140;

fn build_table_loader(rsdp: &mut [u8], tables: &mut [u8], layout: LoaderLayout<'_>) -> Vec<u8> {
    let mut loader = Vec::new();

    // QEMU prepends allocation commands while building; emit the final order
    // directly so all files are allocated before pointer/checksum commands.
    loader.extend(alloc_entry(
        ACPI_RSDP_FILE,
        RSDP_ALLOC_ALIGN,
        LOADER_ZONE_FSEG,
    ));
    loader.extend(alloc_entry(
        ACPI_TABLE_FILE,
        TABLE_ALLOC_ALIGN,
        LOADER_ZONE_HIGH,
    ));

    loader.extend(add_pointer_entry(
        ACPI_TABLE_FILE,
        u32_checked(layout.fadt + u64::from(FADT_X_DSDT_OFFSET)),
        8,
        ACPI_TABLE_FILE,
    ));

    for (idx, _) in layout.xsdt_entries.iter().enumerate() {
        loader.extend(add_pointer_entry(
            ACPI_TABLE_FILE,
            u32_checked(layout.xsdt + ACPI_HEADER_LEN as u64 + (idx as u64) * 8),
            8,
            ACPI_TABLE_FILE,
        ));
    }

    loader.extend(add_pointer_entry(
        ACPI_RSDP_FILE,
        RSDP_XSDT_OFFSET,
        8,
        ACPI_TABLE_FILE,
    ));

    for span in layout.table_spans {
        tables[(span.start + ACPI_CHECKSUM_OFFSET) as usize] = 0;
        loader.extend(add_checksum_entry(
            ACPI_TABLE_FILE,
            span.start + ACPI_CHECKSUM_OFFSET,
            span.start,
            span.len,
        ));
    }

    rsdp[RSDP_V1_CHECKSUM_OFFSET as usize] = 0;
    rsdp[RSDP_EXT_CHECKSUM_OFFSET as usize] = 0;
    loader.extend(add_checksum_entry(
        ACPI_RSDP_FILE,
        RSDP_V1_CHECKSUM_OFFSET,
        0,
        20,
    ));
    loader.extend(add_checksum_entry(
        ACPI_RSDP_FILE,
        RSDP_EXT_CHECKSUM_OFFSET,
        0,
        36,
    ));

    debug_assert_eq!(loader.len() % LOADER_ENTRY_LEN, 0);
    loader
}

fn u32_checked(v: u64) -> u32 {
    u32::try_from(v).expect("ACPI loader offset exceeds 4 GiB")
}

fn loader_entry(command: u32, payload: [u8; LOADER_PAYLOAD_LEN]) -> [u8; LOADER_ENTRY_LEN] {
    let mut entry = [0u8; LOADER_ENTRY_LEN];
    entry[..4].copy_from_slice(&command.to_le_bytes());
    entry[4..].copy_from_slice(&payload);
    entry
}

fn write_loader_name(dst: &mut [u8], name: &str) {
    assert!(
        name.len() < LOADER_FILE_NAME_LEN,
        "loader file name must be < {LOADER_FILE_NAME_LEN} bytes: {name:?}",
    );
    dst[..name.len()].copy_from_slice(name.as_bytes());
}

fn alloc_entry(file: &str, align: u32, zone: u8) -> [u8; LOADER_ENTRY_LEN] {
    let mut payload = [0u8; LOADER_PAYLOAD_LEN];
    write_loader_name(&mut payload[..LOADER_FILE_NAME_LEN], file);
    payload[LOADER_FILE_NAME_LEN..LOADER_FILE_NAME_LEN + 4].copy_from_slice(&align.to_le_bytes());
    payload[LOADER_FILE_NAME_LEN + 4] = zone;
    loader_entry(LOADER_CMD_ALLOCATE, payload)
}

fn add_pointer_entry(
    dest_file: &str,
    offset: u32,
    size: u8,
    src_file: &str,
) -> [u8; LOADER_ENTRY_LEN] {
    assert!(matches!(size, 1 | 2 | 4 | 8), "invalid pointer size {size}");
    let mut payload = [0u8; LOADER_PAYLOAD_LEN];
    write_loader_name(&mut payload[..LOADER_FILE_NAME_LEN], dest_file);
    write_loader_name(
        &mut payload[LOADER_FILE_NAME_LEN..LOADER_FILE_NAME_LEN * 2],
        src_file,
    );
    let off = LOADER_FILE_NAME_LEN * 2;
    payload[off..off + 4].copy_from_slice(&offset.to_le_bytes());
    payload[off + 4] = size;
    loader_entry(LOADER_CMD_ADD_POINTER, payload)
}

fn add_checksum_entry(
    file: &str,
    result_offset: u32,
    start: u32,
    len: u32,
) -> [u8; LOADER_ENTRY_LEN] {
    let mut payload = [0u8; LOADER_PAYLOAD_LEN];
    write_loader_name(&mut payload[..LOADER_FILE_NAME_LEN], file);
    let mut off = LOADER_FILE_NAME_LEN;
    payload[off..off + 4].copy_from_slice(&result_offset.to_le_bytes());
    off += 4;
    payload[off..off + 4].copy_from_slice(&start.to_le_bytes());
    off += 4;
    payload[off..off + 4].copy_from_slice(&len.to_le_bytes());
    loader_entry(LOADER_CMD_ADD_CHECKSUM, payload)
}

/// Serialized length of an XSDT listing `entries` tables (header + 8 bytes each).
fn xsdt_len_for(entries: usize) -> u64 {
    (ACPI_HEADER_LEN + entries * 8) as u64
}

/// RSDP (Root System Description Pointer), ACPI 2.0+ (revision 2). 36 bytes with
/// two checksums: the 20-byte v1 checksum and the full-structure extended one.
fn build_rsdp(xsdt_address: u64) -> Vec<u8> {
    let mut r = Vec::with_capacity(36);
    r.extend_from_slice(b"RSD PTR "); // signature (8 bytes)
    r.push(0); // checksum (v1, patched below)
    r.extend_from_slice(OEM_ID); // OEMID (6 bytes)
    r.push(2); // revision = 2 (ACPI 2.0+)
    r.extend_from_slice(&0u32.to_le_bytes()); // RsdtAddress (unused under XSDT)
    r.extend_from_slice(&36u32.to_le_bytes()); // Length of the whole RSDP
    r.extend_from_slice(&xsdt_address.to_le_bytes()); // XsdtAddress (64-bit)
    r.push(0); // extended checksum (patched below)
    r.extend_from_slice(&[0u8; 3]); // reserved
    debug_assert_eq!(r.len(), 36);

    // v1 checksum covers the first 20 bytes (signature .. RsdtAddress).
    r[8] = 0;
    r[8] = checksum(&r[..20]);
    // Extended checksum covers the entire 36-byte structure.
    r[32] = 0;
    r[32] = checksum(&r);
    r
}

/// XSDT (Extended System Description Table): header + a 64-bit pointer per table.
fn build_xsdt(entries: &[u64]) -> Vec<u8> {
    let mut t = Table::new(b"XSDT", 1);
    for &addr in entries {
        t.u64(addr);
    }
    t.finish()
}

/// FADT (Fixed ACPI Description Table), revision 6. Hardware-reduced ACPI with
/// PSCI-via-HVC declared through the ARM boot flags; `X_Dsdt` points at the DSDT.
/// `LOW_POWER_S0_IDLE_CAPABLE` stays clear because BridgeVM has no platform
/// power engine/idle implementation, so advertising that platform contract
/// would describe power-management support the VMM does not provide.
fn build_fadt(dsdt_address: u64) -> Vec<u8> {
    let mut t = Table::new(b"FACP", 6); // FADT signature is "FACP"
    t.u32(0); // FIRMWARE_CTRL (FACS) — none under HW-reduced ACPI
    t.u32(0); // DSDT (32-bit) — superseded by X_DSDT below
    t.u8(0); // reserved (was INT_MODEL in ACPI 1.0)
    t.u8(0); // Preferred_PM_Profile = unspecified
    t.u16(0); // SCI_INT
    t.u32(0); // SMI_CMD
    t.u8(0); // ACPI_ENABLE
    t.u8(0); // ACPI_DISABLE
    t.u8(0); // S4BIOS_REQ
    t.u8(0); // PSTATE_CNT
    t.u32(0); // PM1a_EVT_BLK
    t.u32(0); // PM1b_EVT_BLK
    t.u32(0); // PM1a_CNT_BLK
    t.u32(0); // PM1b_CNT_BLK
    t.u32(0); // PM2_CNT_BLK
    t.u32(0); // PM_TMR_BLK
    t.u32(0); // GPE0_BLK
    t.u32(0); // GPE1_BLK
    t.u8(0); // PM1_EVT_LEN
    t.u8(0); // PM1_CNT_LEN
    t.u8(0); // PM2_CNT_LEN
    t.u8(0); // PM_TMR_LEN
    t.u8(0); // GPE0_BLK_LEN
    t.u8(0); // GPE1_BLK_LEN
    t.u8(0); // GPE1_BASE
    t.u8(0); // CST_CNT
    t.u16(0); // P_LVL2_LAT
    t.u16(0); // P_LVL3_LAT
    t.u16(0); // FLUSH_SIZE
    t.u16(0); // FLUSH_STRIDE
    t.u8(0); // DUTY_OFFSET
    t.u8(0); // DUTY_WIDTH
    t.u8(0); // DAY_ALRM
    t.u8(0); // MON_ALRM
    t.u8(0); // CENTURY
    t.u16(0); // IAPC_BOOT_ARCH (x86-only, zero on ARM)
    t.u8(0); // reserved
    t.u32(FADT_FLAG_HW_REDUCED_ACPI); // Flags
    t.gas_null(); // RESET_REG
    t.u8(0); // RESET_VALUE
    t.u16(FADT_ARM_BOOT_PSCI_COMPLIANT | FADT_ARM_BOOT_PSCI_USE_HVC); // ARM_BOOT_ARCH
    t.u8(0); // FADT Minor Version
    t.u64(0); // X_FIRMWARE_CTRL
    t.u64(dsdt_address); // X_DSDT (64-bit pointer to the DSDT)
    t.gas_null(); // X_PM1a_EVT_BLK
    t.gas_null(); // X_PM1b_EVT_BLK
    t.gas_null(); // X_PM1a_CNT_BLK
    t.gas_null(); // X_PM1b_CNT_BLK
    t.gas_null(); // X_PM2_CNT_BLK
    t.gas_null(); // X_PM_TMR_BLK
    t.gas_null(); // X_GPE0_BLK
    t.gas_null(); // X_GPE1_BLK
    t.gas_null(); // SLEEP_CONTROL_REG
    t.gas_null(); // SLEEP_STATUS_REG
    t.u64(0); // Hypervisor Vendor Identity
    t.finish()
}

/// Length of the FADT this module emits (header + the fields appended above).
fn fadt_len() -> u64 {
    // Built once to keep the layout single-sourced; cheap enough at startup.
    build_fadt(0).len() as u64
}

// ---- AML helpers ------------------------------------------------------------

const AML_ZERO_OP: u8 = 0x00;
const AML_ONE_OP: u8 = 0x01;
const AML_BYTE_PREFIX: u8 = 0x0A;
const AML_DWORD_PREFIX: u8 = 0x0C;
const AML_STRING_PREFIX: u8 = 0x0D;
const AML_NAME_OP: u8 = 0x08;
const AML_SCOPE_OP: u8 = 0x10;
const AML_BUFFER_OP: u8 = 0x11;
const AML_METHOD_OP: u8 = 0x14;
const AML_EXT_OP: u8 = 0x5B;
const AML_DEVICE_OP: u8 = 0x82;
const AML_LOCAL0_OP: u8 = 0x60;
const AML_ARG0_OP: u8 = 0x68;
const AML_STORE_OP: u8 = 0x70;
const AML_AND_OP: u8 = 0x7B;
const AML_OR_OP: u8 = 0x7D;
const AML_CREATE_DWORD_FIELD_OP: u8 = 0x8A;
const AML_LNOT_OP: u8 = 0x92;
const AML_LEQUAL_OP: u8 = 0x93;
const AML_IF_OP: u8 = 0xA0;
const AML_ELSE_OP: u8 = 0xA1;
const AML_RETURN_OP: u8 = 0xA4;

const EISA_PNP0A08: [u8; 4] = [0x41, 0xD0, 0x0A, 0x08];
const EISA_PNP0A03: [u8; 4] = [0x41, 0xD0, 0x0A, 0x03];
const EISA_PNP0C02: [u8; 4] = [0x41, 0xD0, 0x0C, 0x02];
const EISA_PNP0C0C: [u8; 4] = [0x41, 0xD0, 0x0C, 0x0C];
const PCI_HOST_BRIDGE_OSC_UUID: [u8; 16] = [
    0x5B, 0x4D, 0xDB, 0x33, 0xF7, 0x1F, 0x1C, 0x40, 0x96, 0x57, 0x74, 0x41, 0xC0, 0x3D, 0xD7, 0x66,
];

fn aml_pkg_length(payload_len: usize) -> Vec<u8> {
    for encoded_len in 1..=4 {
        let total = payload_len + encoded_len;
        if encoded_len == 1 {
            if total <= 0x3F {
                return vec![total as u8];
            }
            continue;
        }
        let max = match encoded_len {
            2 => 0x0FFF,
            3 => 0x0F_FFFF,
            4 => 0x0FFF_FFFF,
            _ => unreachable!(),
        };
        if total <= max {
            let follow = encoded_len - 1;
            let mut out = Vec::with_capacity(encoded_len);
            out.push(((follow as u8) << 6) | ((total & 0x0F) as u8));
            let mut rest = total >> 4;
            for _ in 0..follow {
                out.push((rest & 0xFF) as u8);
                rest >>= 8;
            }
            return out;
        }
    }
    panic!("AML package length too large: {payload_len}");
}

fn aml_name_string(name: &[u8; 4], value: &str) -> Vec<u8> {
    assert!(
        !value.as_bytes().contains(&0),
        "AML strings are NUL-terminated"
    );
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.push(AML_STRING_PREFIX);
    out.extend_from_slice(value.as_bytes());
    out.push(0);
    out
}

fn aml_name_eisa(name: &[u8; 4], encoded: [u8; 4]) -> Vec<u8> {
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.push(AML_DWORD_PREFIX);
    out.extend_from_slice(&encoded);
    out
}

fn aml_name_simple(name: &[u8; 4], op: u8) -> Vec<u8> {
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.push(op);
    out
}

fn aml_name_byte(name: &[u8; 4], value: u8) -> Vec<u8> {
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.extend(aml_byte(value));
    out
}

fn aml_name_buffer(name: &[u8; 4], bytes: &[u8]) -> Vec<u8> {
    assert!(bytes.len() <= u8::MAX as usize, "small AML buffer expected");
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.push(AML_BUFFER_OP);
    out.extend(aml_pkg_length(2 + bytes.len()));
    out.push(AML_BYTE_PREFIX);
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
    out
}

fn aml_name_ref(name: &[u8; 4]) -> Vec<u8> {
    name.to_vec()
}

fn aml_arg(n: u8) -> Vec<u8> {
    assert!(n <= 6, "AML has only Arg0..Arg6");
    vec![AML_ARG0_OP + n]
}

fn aml_local0() -> Vec<u8> {
    vec![AML_LOCAL0_OP]
}

fn aml_byte(value: u8) -> Vec<u8> {
    match value {
        0 => vec![AML_ZERO_OP],
        1 => vec![AML_ONE_OP],
        _ => vec![AML_BYTE_PREFIX, value],
    }
}

fn aml_uuid_buffer(bytes: &[u8; 16]) -> Vec<u8> {
    let mut out = vec![AML_BUFFER_OP];
    out.extend(aml_pkg_length(2 + bytes.len()));
    out.push(AML_BYTE_PREFIX);
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
    out
}

fn aml_create_dword_field(source: &[u8], byte_index: u8, name: &[u8; 4]) -> Vec<u8> {
    let mut out = vec![AML_CREATE_DWORD_FIELD_OP];
    out.extend_from_slice(source);
    out.extend(aml_byte(byte_index));
    out.extend_from_slice(name);
    out
}

fn aml_store(source: &[u8], target: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_STORE_OP];
    out.extend_from_slice(source);
    out.extend_from_slice(target);
    out
}

fn aml_binary_op(op: u8, left: &[u8], right: &[u8], target: &[u8]) -> Vec<u8> {
    let mut out = vec![op];
    out.extend_from_slice(left);
    out.extend_from_slice(right);
    out.extend_from_slice(target);
    out
}

fn aml_equal(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_LEQUAL_OP];
    out.extend_from_slice(left);
    out.extend_from_slice(right);
    out
}

fn aml_not_equal(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_LNOT_OP];
    out.extend(aml_equal(left, right));
    out
}

fn aml_if(predicate: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_IF_OP];
    out.extend(aml_pkg_length(predicate.len() + body.len()));
    out.extend_from_slice(predicate);
    out.extend_from_slice(body);
    out
}

fn aml_else(body: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_ELSE_OP];
    out.extend(aml_pkg_length(body.len()));
    out.extend_from_slice(body);
    out
}

fn aml_return(value: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_RETURN_OP];
    out.extend_from_slice(value);
    out
}

fn aml_scope(name: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_SCOPE_OP];
    out.extend(aml_pkg_length(name.len() + body.len()));
    out.extend_from_slice(name);
    out.extend_from_slice(body);
    out
}

fn aml_device(name: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_EXT_OP, AML_DEVICE_OP];
    out.extend(aml_pkg_length(name.len() + body.len()));
    out.extend_from_slice(name);
    out.extend_from_slice(body);
    out
}

fn aml_method(name: &[u8; 4], arg_count: u8, serialized: bool, body: &[u8]) -> Vec<u8> {
    assert!(
        arg_count <= 7,
        "AML methods support at most seven arguments"
    );
    let mut out = vec![AML_METHOD_OP];
    out.extend(aml_pkg_length(name.len() + 1 + body.len()));
    out.extend_from_slice(name);
    out.push(arg_count | if serialized { 0x08 } else { 0x00 });
    out.extend_from_slice(body);
    out
}

fn resource_memory32_fixed(base: u64, size: u64) -> Vec<u8> {
    let base = u32::try_from(base).expect("Memory32Fixed base exceeds 32 bits");
    let size = u32::try_from(size).expect("Memory32Fixed size exceeds 32 bits");
    let mut out = vec![0x86, 0x09, 0x00, 0x01]; // Memory32Fixed, ReadWrite
    out.extend_from_slice(&base.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out
}

fn resource_interrupt(gsiv: u32) -> Vec<u8> {
    let mut out = vec![0x89, 0x06, 0x00, 0x01, 0x01]; // Consumer, level, active-high, exclusive
    out.extend_from_slice(&gsiv.to_le_bytes());
    out
}

fn resource_word_bus_number(min: u16, max: u16) -> Vec<u8> {
    let len = max
        .checked_sub(min)
        .and_then(|v| v.checked_add(1))
        .expect("invalid PCI bus range");
    let mut out = vec![0x88, 0x0D, 0x00, 0x02, 0x0C, 0x00]; // Word address, bus, min/max fixed
    out.extend_from_slice(&0u16.to_le_bytes()); // granularity
    out.extend_from_slice(&min.to_le_bytes());
    out.extend_from_slice(&max.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // translation offset
    out.extend_from_slice(&len.to_le_bytes());
    out
}

fn resource_dword_memory(base: u64, size: u64) -> Vec<u8> {
    let base = u32::try_from(base).expect("DWordMemory base exceeds 32 bits");
    let size = u32::try_from(size).expect("DWordMemory size exceeds 32 bits");
    let max = base
        .checked_add(size)
        .and_then(|v| v.checked_sub(1))
        .expect("DWordMemory range overflow");
    let mut out = vec![0x87, 0x17, 0x00, 0x00, 0x0C, 0x01]; // Memory, min/max fixed, read-write
    out.extend_from_slice(&0u32.to_le_bytes()); // granularity
    out.extend_from_slice(&base.to_le_bytes());
    out.extend_from_slice(&max.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // translation offset
    out.extend_from_slice(&size.to_le_bytes());
    out
}

fn resource_dword_io(base: u64, size: u64) -> Vec<u8> {
    let base = u32::try_from(base).expect("DWordIO translation exceeds 32 bits");
    let size = u32::try_from(size).expect("DWordIO size exceeds 32 bits");
    let max = size.checked_sub(1).expect("DWordIO size must be non-zero");
    let mut out = vec![0x87, 0x17, 0x00, 0x01, 0x0C, 0x03]; // I/O, min/max fixed, entire range
    out.extend_from_slice(&0u32.to_le_bytes()); // granularity
    out.extend_from_slice(&0u32.to_le_bytes()); // host-visible I/O min
    out.extend_from_slice(&max.to_le_bytes()); // host-visible I/O max
    out.extend_from_slice(&base.to_le_bytes()); // translation to CPU MMIO window
    out.extend_from_slice(&size.to_le_bytes());
    out
}

fn resource_qword_memory(base: u64, size: u64) -> Vec<u8> {
    let max = base
        .checked_add(size)
        .and_then(|v| v.checked_sub(1))
        .expect("QWordMemory range overflow");
    let mut out = vec![0x8A, 0x2B, 0x00, 0x00, 0x0C, 0x01]; // Memory, min/max fixed, read-write
    out.extend_from_slice(&0u64.to_le_bytes()); // granularity
    out.extend_from_slice(&base.to_le_bytes());
    out.extend_from_slice(&max.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes()); // translation offset
    out.extend_from_slice(&size.to_le_bytes());
    out
}

fn resource_end_tag() -> [u8; 2] {
    [0x79, 0x00]
}

fn build_pl011_dsdt_device() -> Vec<u8> {
    let mut crs = Vec::new();
    crs.extend(resource_memory32_fixed(
        machine::UART.base,
        machine::UART.size,
    ));
    crs.extend(resource_interrupt(machine::spi_to_intid(machine::SPI_UART)));
    crs.extend(resource_end_tag());

    let mut body = Vec::new();
    body.extend(aml_name_string(b"_HID", "ARMH0011"));
    body.extend(aml_name_simple(b"_UID", AML_ZERO_OP));
    body.extend(aml_name_simple(b"_CCA", AML_ONE_OP));
    body.extend(aml_name_buffer(b"_CRS", &crs));
    aml_device(b"COM0", &body)
}

fn build_cpu_dsdt_device(cpu: u64) -> Vec<u8> {
    let name = format!("C{cpu:03X}");
    assert!(
        name.len() == 4,
        "ACPI CPU device name requires a three-hex-digit UID: {cpu}",
    );
    let mut device_name = [0u8; 4];
    device_name.copy_from_slice(name.as_bytes());
    let uid = u8::try_from(cpu).expect("ACPI CPU UID exceeds one-byte AML encoding");

    let mut body = Vec::new();
    body.extend(aml_name_string(b"_HID", "ACPI0007"));
    body.extend(aml_name_byte(b"_UID", uid));
    aml_device(&device_name, &body)
}

fn build_pci_root_dsdt_device() -> Vec<u8> {
    let mut crs = Vec::new();
    crs.extend(resource_word_bus_number(0, 0x00FF));
    crs.extend(resource_dword_memory(
        machine::PCIE_MMIO_32.base,
        machine::PCIE_MMIO_32.size,
    ));
    crs.extend(resource_qword_memory(
        machine::PCIE_MMIO_64.base,
        machine::PCIE_MMIO_64.size,
    ));
    crs.extend(resource_dword_io(
        machine::PCIE_PIO.base,
        machine::PCIE_PIO.size,
    ));
    crs.extend(resource_end_tag());

    let mut body = Vec::new();
    body.extend(aml_name_eisa(b"_HID", EISA_PNP0A08));
    body.extend(aml_name_eisa(b"_CID", EISA_PNP0A03));
    body.extend(aml_name_simple(b"_SEG", AML_ZERO_OP));
    body.extend(aml_name_simple(b"_BBN", AML_ZERO_OP));
    body.extend(aml_name_simple(b"_UID", AML_ZERO_OP));
    body.extend(aml_name_simple(b"_CCA", AML_ONE_OP));
    body.extend(aml_name_buffer(b"_CRS", &crs));
    body.extend(build_pci_root_osc_method());
    aml_device(b"PCI0", &body)
}

fn build_pci_root_osc_method() -> Vec<u8> {
    let cdw1 = aml_name_ref(b"CDW1");
    let cdw3 = aml_name_ref(b"CDW3");
    let local0 = aml_local0();

    let mut known_uuid_body = Vec::new();
    known_uuid_body.extend(aml_create_dword_field(&aml_arg(3), 4, b"CDW2"));
    known_uuid_body.extend(aml_create_dword_field(&aml_arg(3), 8, b"CDW3"));
    known_uuid_body.extend(aml_store(&cdw3, &local0));
    // Match QEMU's generic PCI host bridge policy: grant OS control for
    // PCIeHotplug, SHPCHotplug, PME, AER and PCIeCapability, but not LTR.
    known_uuid_body.extend(aml_binary_op(AML_AND_OP, &local0, &aml_byte(0x1F), &local0));
    known_uuid_body.extend(aml_if(
        &aml_not_equal(&aml_arg(1), &aml_byte(1)),
        &aml_binary_op(AML_OR_OP, &cdw1, &aml_byte(0x08), &cdw1),
    ));
    known_uuid_body.extend(aml_if(
        &aml_not_equal(&cdw3, &local0),
        &aml_binary_op(AML_OR_OP, &cdw1, &aml_byte(0x10), &cdw1),
    ));
    known_uuid_body.extend(aml_store(&local0, &cdw3));

    let mut unknown_uuid_body = Vec::new();
    unknown_uuid_body.extend(aml_binary_op(AML_OR_OP, &cdw1, &aml_byte(0x04), &cdw1));

    let mut body = Vec::new();
    body.extend(aml_create_dword_field(&aml_arg(3), 0, b"CDW1"));
    body.extend(aml_if(
        &aml_equal(&aml_arg(0), &aml_uuid_buffer(&PCI_HOST_BRIDGE_OSC_UUID)),
        &known_uuid_body,
    ));
    body.extend(aml_else(&unknown_uuid_body));
    body.extend(aml_return(&aml_arg(3)));
    aml_method(b"_OSC", 4, false, &body)
}

fn build_power_button_dsdt_device() -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(aml_name_eisa(b"_HID", EISA_PNP0C0C));
    aml_device(b"PWRB", &body)
}

fn build_ecam_reserved_dsdt_device() -> Vec<u8> {
    let mut crs = Vec::new();
    crs.extend(resource_qword_memory(
        machine::PCIE_ECAM.base,
        machine::PCIE_ECAM.size,
    ));
    crs.extend(resource_end_tag());

    let mut body = Vec::new();
    body.extend(aml_name_eisa(b"_HID", EISA_PNP0C02));
    body.extend(aml_name_simple(b"_UID", AML_ZERO_OP));
    body.extend(aml_name_buffer(b"_CRS", &crs));
    aml_device(b"RES0", &body)
}

/// QEMU-like DSDT surface for devices Linux/Windows enumerate through ACPI.
/// MADT/GTDT/MCFG/SPCR/DBG2 carry the architectural tables, while this AML names
/// the platform devices the OS driver core expects to bind (`ACPI0007` CPU
/// devices, `ARMH0011` PL011, `PNP0A08` PCI root bridge and a power button).
fn build_dsdt(cpu_count: u64) -> Vec<u8> {
    let mut sb = Vec::new();
    for cpu in 0..cpu_count {
        sb.extend(build_cpu_dsdt_device(cpu));
    }
    sb.extend(build_pl011_dsdt_device());
    sb.extend(build_pci_root_dsdt_device());
    sb.extend(build_ecam_reserved_dsdt_device());
    sb.extend(build_power_button_dsdt_device());

    let mut t = Table::new(b"DSDT", 2);
    t.bytes.extend(aml_scope(b"_SB_", &sb));
    t.finish()
}

/// MADT (Multiple APIC Description Table) for GICv3: one GICC per CPU, a single
/// GICD, and a GICR covering the whole redistributor window.
fn build_madt(cpu_count: u64) -> Vec<u8> {
    let mut t = Table::new(b"APIC", 4); // MADT signature is "APIC"
    t.u32(0); // Local Interrupt Controller Address (unused on GICv3)
    t.u32(0); // Flags (no PC-AT 8259)

    // GIC Distributor (type 0x0C) — exactly one. QEMU emits the GICD before
    // per-CPU GICC structures.
    t.u8(0x0C); // Type = GICD
    t.u8(24); // Length
    t.u16(0); // reserved
    t.u32(0); // GIC ID
    t.u64(machine::GIC_DIST.base); // Physical Base Address
    t.u32(0); // System Vector Base (reserved, must be 0)
    t.u8(3); // GIC version = 3
    t.pad(3); // reserved

    // One GIC CPU Interface (type 0x0B) structure per CPU.
    for cpu in 0..cpu_count {
        t.u8(0x0B); // Type = GICC
        t.u8(80); // Length
        t.u16(0); // reserved
        t.u32(cpu as u32); // CPU Interface Number
        t.u32(cpu as u32); // ACPI Processor UID
        t.u32(1); // Flags = Enabled
        t.u32(0); // Parking Protocol Version
        t.u32(ppi_to_gsiv(machine::PPI_PMU)); // Performance Interrupt GSIV
        t.u64(0); // Parked Address
        t.u64(0); // Physical Base Address (0 on GICv3 — sysreg interface)
        t.u64(0); // GICV (virtual CPU interface)
        t.u64(0); // GICH (hypervisor interface)
        t.u32(0); // VGIC Maintenance interrupt
        t.u64(redistributor_base(cpu)); // GICR Base Address (per-CPU)
                                        // MPIDR: affinity packed Aff3[39:32] | Aff2[23:16] | Aff1[15:8] | Aff0[7:0].
        t.u64(mpidr_for(cpu));
        t.u8(0); // Processor Power Efficiency Class
        t.u8(0); // reserved
        t.u16(0); // SPE overflow Interrupt
    }

    // GIC Redistributor (type 0x0E) — discovery range covering all GICRs.
    t.u8(0x0E); // Type = GICR
    t.u8(16); // Length
    t.u16(0); // reserved
    t.u64(machine::GIC_REDIST.base); // Discovery Range Base Address
    t.u32(machine::GIC_REDIST.size as u32); // Discovery Range Length

    // Generic MSI Frame (type 0x0D) — Apple `hv_gic` exposes message-signalled
    // SPIs through a GICv2m-compatible frame, not an architectural ITS/LPI
    // block. Linux's GICv3 driver only probes ITS when the distributor advertises
    // LPIs; the Apple GIC does not, so the ACPI MSI contract must be this frame.
    t.u8(0x0D); // Type = Generic MSI Frame
    t.u8(24); // Length
    t.u16(0); // reserved
    t.u32(0); // MSI Frame ID
    t.u64(machine::GIC_MSI_FRAME.base); // Physical Base Address
    t.u32(1); // Flags: override SPI values below
    t.u16(machine::GIC_MSI_INTID_COUNT as u16); // SPI Count
    t.u16(machine::GIC_MSI_INTID_BASE as u16); // SPI Base (absolute INTID)

    t.finish()
}

/// Per-CPU GICR base = window base + cpu * stride.
fn redistributor_base(cpu: u64) -> u64 {
    machine::GIC_REDIST.base + cpu * machine::GICV3_REDIST_STRIDE
}

/// Linear MPIDR affinity for `cpu` (Aff0 = 0..15, Aff1 = group of 16), matching
/// the scheme QEMU `virt` uses for small CPU counts.
fn mpidr_for(cpu: u64) -> u64 {
    machine::cpu_mpidr(cpu)
}

const PPTT_NODE_PROCESSOR: u8 = 0;
const PPTT_PROCESSOR_PHYSICAL_PACKAGE: u32 = 1 << 0;
const PPTT_PROCESSOR_ACPI_ID_VALID: u32 = 1 << 1;
const PPTT_PROCESSOR_LEAF: u32 = 1 << 3;
const PPTT_PROCESSOR_IDENTICAL: u32 = 1 << 4;

/// Append an ACPI PPTT processor hierarchy node (type 0). Offsets stored in
/// `parent` and `private_resources` are relative to the start of the PPTT table.
fn append_pptt_processor_node(
    t: &mut Table,
    flags: u32,
    parent: u32,
    acpi_processor_id: u32,
    private_resources: &[u32],
) {
    let len = 20 + private_resources.len() * 4;
    let len = u8::try_from(len).expect("PPTT processor node length exceeds u8");
    t.u8(PPTT_NODE_PROCESSOR);
    t.u8(len);
    t.u16(0); // reserved
    t.u32(flags);
    t.u32(parent);
    t.u32(acpi_processor_id);
    t.u32(private_resources.len() as u32);
    for &resource in private_resources {
        t.u32(resource);
    }
}

/// PPTT (Processor Properties Topology Table). Match QEMU's simple homogeneous
/// topology for `virt`: one root package node, one socket node, and one leaf
/// processor node per ACPI Processor UID.
fn build_pptt(cpu_count: u64) -> Vec<u8> {
    let mut t = Table::new(b"PPTT", 2);

    let root_offset = t.bytes.len() as u32;
    append_pptt_processor_node(
        &mut t,
        PPTT_PROCESSOR_PHYSICAL_PACKAGE | PPTT_PROCESSOR_IDENTICAL,
        0,
        0,
        &[],
    );

    let socket_offset = t.bytes.len() as u32;
    append_pptt_processor_node(
        &mut t,
        PPTT_PROCESSOR_PHYSICAL_PACKAGE | PPTT_PROCESSOR_IDENTICAL,
        root_offset,
        0,
        &[],
    );

    for cpu in 0..cpu_count {
        append_pptt_processor_node(
            &mut t,
            PPTT_PROCESSOR_ACPI_ID_VALID | PPTT_PROCESSOR_LEAF,
            socket_offset,
            u32::try_from(cpu).expect("ACPI processor ID exceeds u32"),
            &[],
        );
    }

    t.finish()
}

/// GTDT (Generic Timer Description Table) describing the architected timer. The
/// per-CPU timer interrupts are PPIs; the GSIV is `PPI + 16` (PPIs occupy
/// INTIDs 16..31). Edge/level is encoded in the per-timer flags.
fn build_gtdt() -> Vec<u8> {
    /// GTDT timer flag: interrupt is level-triggered (bit 1 clear = level).
    const TIMER_FLAG_LEVEL: u32 = 0;

    let mut t = Table::new(b"GTDT", 2);
    t.u64(0xFFFF_FFFF_FFFF_FFFF); // CntControlBase — not memory-mapped
    t.u32(0); // reserved
    t.u32(ppi_to_gsiv(machine::PPI_TIMER_SECURE)); // Secure EL1 timer GSIV
    t.u32(TIMER_FLAG_LEVEL); // Secure EL1 timer flags
    t.u32(ppi_to_gsiv(machine::PPI_TIMER_NONSEC)); // Non-Secure EL1 timer GSIV
    t.u32(TIMER_FLAG_LEVEL); // Non-Secure EL1 timer flags
    t.u32(ppi_to_gsiv(machine::PPI_TIMER_VIRT)); // Virtual EL1 timer GSIV
    t.u32(TIMER_FLAG_LEVEL); // Virtual EL1 timer flags
    t.u32(ppi_to_gsiv(machine::PPI_TIMER_HYP)); // EL2 (hypervisor) timer GSIV
    t.u32(TIMER_FLAG_LEVEL); // EL2 timer flags
    t.u64(0xFFFF_FFFF_FFFF_FFFF); // CntReadBase — not memory-mapped
    t.u32(0); // Platform Timer Count
    t.u32(0); // Platform Timer Offset (none present)
    t.finish()
}

/// PPI number to its absolute GIC interrupt ID (GSIV). PPIs occupy INTIDs 16..31.
fn ppi_to_gsiv(ppi: u32) -> u32 {
    ppi + 16
}

/// MCFG (PCI memory-mapped configuration space) describing the ECAM window for
/// PCI segment 0, buses 0..=255.
fn build_mcfg() -> Vec<u8> {
    let mut t = Table::new(b"MCFG", 1);
    t.u64(0); // reserved
              // One configuration-space allocation entry (16 bytes).
    t.u64(machine::PCIE_ECAM.base); // Base Address of enhanced config space
    t.u16(0); // PCI Segment Group Number
    t.u8(0); // Start PCI bus number
    t.u8(0xFF); // End PCI bus number
    t.u32(0); // reserved
    t.finish()
}

/// SPCR (Serial Port Console Redirection) pointing the OS console at the PL011.
fn build_spcr() -> Vec<u8> {
    let mut t = Table::new(b"SPCR", 2);
    t.u8(0x03); // Interface Type = ARM PL011 UART
    t.pad(3); // reserved
    t.gas_memory_with_access_size(machine::UART.base, 32, 3); // Base Address (dword access)
    t.u8(0x08); // Interrupt Type = ARM GIC
    t.u8(0); // IRQ (8259, unused)
    t.u32(machine::spi_to_intid(machine::SPI_UART)); // Global System Interrupt
    t.u8(7); // Baud Rate = as-is (do not reconfigure)
    t.u8(0); // Parity = none
    t.u8(1); // Stop Bits = 1
    t.u8(0); // Flow Control = none
    t.u8(0); // Terminal Type = VT100
    t.u8(0); // Language (reserved)
    t.u16(0xFFFF); // PCI Device ID = not a PCI device
    t.u16(0xFFFF); // PCI Vendor ID = not a PCI device
    t.u8(0); // PCI Bus
    t.u8(0); // PCI Device
    t.u8(0); // PCI Function
    t.u32(0); // PCI Flags
    t.u8(0); // PCI Segment
    t.u32(0); // reserved
    t.finish()
}

/// DBG2 (Debug Port Table 2) describing the same PL011 as a serial debug port.
fn build_dbg2() -> Vec<u8> {
    const NAMESPACE: &[u8] = b"COM0\0";
    const DEBUG_DEVICE_INFO_OFFSET: u32 = 44;
    const BASE_ADDRESS_REGISTER_OFFSET: u16 = 22;
    const ADDRESS_SIZE_OFFSET: u16 = 34;
    const NAMESPACE_STRING_OFFSET: u16 = 38;

    let device_len =
        u16::try_from(BASE_ADDRESS_REGISTER_OFFSET as usize + 12 + 4 + NAMESPACE.len())
            .expect("DBG2 device info length exceeds u16");

    let mut t = Table::new(b"DBG2", 0);
    t.u32(DEBUG_DEVICE_INFO_OFFSET); // OffsetDbgDeviceInfo
    t.u32(1); // NumberDbgDeviceInfo

    t.u8(0); // Revision
    t.u16(device_len); // Length
    t.u8(1); // NumberOfGenericAddressRegisters
    t.u16(NAMESPACE.len() as u16); // NameSpaceStringLength
    t.u16(NAMESPACE_STRING_OFFSET); // NameSpaceStringOffset
    t.u16(0); // OemDataLength
    t.u16(0); // OemDataOffset
    t.u16(0x8000); // Port Type = Serial
    t.u16(0x0003); // Port Subtype = ARM PL011 UART
    t.u16(0); // Reserved
    t.u16(BASE_ADDRESS_REGISTER_OFFSET);
    t.u16(ADDRESS_SIZE_OFFSET);

    t.gas_memory_with_access_size(machine::UART.base, 32, 3);
    t.u32(machine::UART.size as u32);
    t.bytes.extend_from_slice(NAMESPACE);
    t.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sum of every byte must be zero (mod 256) for any valid ACPI structure.
    fn sums_to_zero(bytes: &[u8]) -> bool {
        bytes.iter().fold(0u8, |a, &b| a.wrapping_add(b)) == 0
    }

    /// Read a little-endian u16 at `off`.
    fn le16(b: &[u8], off: usize) -> u16 {
        u16::from_le_bytes([b[off], b[off + 1]])
    }
    /// Read a little-endian u32 at `off`.
    fn le32(b: &[u8], off: usize) -> u32 {
        u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
    }
    /// Read a little-endian u64 at `off`.
    fn le64(b: &[u8], off: usize) -> u64 {
        u64::from_le_bytes([
            b[off],
            b[off + 1],
            b[off + 2],
            b[off + 3],
            b[off + 4],
            b[off + 5],
            b[off + 6],
            b[off + 7],
        ])
    }
    fn le_name(b: &[u8], off: usize) -> String {
        let name = &b[off..off + LOADER_FILE_NAME_LEN];
        let len = name
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(LOADER_FILE_NAME_LEN);
        std::str::from_utf8(&name[..len]).unwrap().to_string()
    }
    fn read_le_sized(b: &[u8], off: usize, size: u8) -> u64 {
        let mut raw = [0u8; 8];
        raw[..size as usize].copy_from_slice(&b[off..off + size as usize]);
        u64::from_le_bytes(raw)
    }
    fn write_le_sized(b: &mut [u8], off: usize, size: u8, value: u64) {
        b[off..off + size as usize].copy_from_slice(&value.to_le_bytes()[..size as usize]);
    }
    fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }
    fn replay_loader(blobs: &AcpiBlobs) -> (Vec<u8>, Vec<u8>) {
        const TABLES_BASE: u64 = 0x4800_0000;
        const RSDP_BASE: u64 = 0x000F_0000;

        let mut tables = Vec::new();
        let mut rsdp = Vec::new();

        for entry in blobs.loader.chunks_exact(LOADER_ENTRY_LEN) {
            let cmd = le32(entry, 0);
            match cmd {
                LOADER_CMD_ALLOCATE => {
                    let file = le_name(entry, 4);
                    let align = le32(entry, 4 + LOADER_FILE_NAME_LEN);
                    let zone = entry[4 + LOADER_FILE_NAME_LEN + 4];
                    match file.as_str() {
                        ACPI_RSDP_FILE => {
                            assert_eq!(align, RSDP_ALLOC_ALIGN);
                            assert_eq!(zone, LOADER_ZONE_FSEG);
                            rsdp = blobs.rsdp.clone();
                        }
                        ACPI_TABLE_FILE => {
                            assert_eq!(align, TABLE_ALLOC_ALIGN);
                            assert_eq!(zone, LOADER_ZONE_HIGH);
                            tables = blobs.tables.clone();
                        }
                        other => panic!("unexpected ACPI allocation {other}"),
                    }
                }
                LOADER_CMD_ADD_POINTER => {
                    let pointer_file = le_name(entry, 4);
                    let pointee_file = le_name(entry, 4 + LOADER_FILE_NAME_LEN);
                    let off = 4 + LOADER_FILE_NAME_LEN * 2;
                    let pointer_offset = le32(entry, off) as usize;
                    let pointer_size = entry[off + 4];
                    let (pointee_base, pointee_size) = match pointee_file.as_str() {
                        ACPI_TABLE_FILE => (TABLES_BASE, tables.len()),
                        ACPI_RSDP_FILE => (RSDP_BASE, rsdp.len()),
                        other => panic!("unexpected pointer source {other}"),
                    };
                    let target = match pointer_file.as_str() {
                        ACPI_TABLE_FILE => &mut tables,
                        ACPI_RSDP_FILE => &mut rsdp,
                        other => panic!("unexpected pointer destination {other}"),
                    };
                    let value = read_le_sized(target, pointer_offset, pointer_size);
                    assert!(
                        value < pointee_size as u64,
                        "pointer offset {value:#x} must point inside {pointee_file}",
                    );
                    write_le_sized(target, pointer_offset, pointer_size, value + pointee_base);
                }
                LOADER_CMD_ADD_CHECKSUM => {
                    let file = le_name(entry, 4);
                    let off = 4 + LOADER_FILE_NAME_LEN;
                    let result = le32(entry, off) as usize;
                    let start = le32(entry, off + 4) as usize;
                    let len = le32(entry, off + 8) as usize;
                    let target = match file.as_str() {
                        ACPI_TABLE_FILE => &mut tables,
                        ACPI_RSDP_FILE => &mut rsdp,
                        other => panic!("unexpected checksum file {other}"),
                    };
                    target[result] = checksum(&target[start..start + len]);
                }
                other => panic!("unexpected ACPI loader command {other}"),
            }
        }

        assert!(!tables.is_empty(), "table allocation command missing");
        assert!(!rsdp.is_empty(), "RSDP allocation command missing");
        (rsdp, tables)
    }

    /// Split the `etc/acpi/tables` blob back into (signature, slice) tables by
    /// walking each header's length field.
    fn split_tables(tables: &[u8]) -> Vec<(String, &[u8])> {
        let mut out = Vec::new();
        let mut off = 0usize;
        while off + ACPI_HEADER_LEN <= tables.len() {
            let sig = String::from_utf8_lossy(&tables[off..off + 4]).to_string();
            let len = le32(tables, off + 4) as usize;
            assert!(
                len >= ACPI_HEADER_LEN,
                "table {sig} length too small: {len}"
            );
            assert!(off + len <= tables.len(), "table {sig} overruns blob");
            out.push((sig, &tables[off..off + len]));
            off += len;
        }
        assert_eq!(off, tables.len(), "tables blob has trailing bytes");
        out
    }

    fn find<'a>(tables: &'a [(String, &'a [u8])], sig: &str) -> &'a [u8] {
        tables
            .iter()
            .find(|(s, _)| s == sig)
            .unwrap_or_else(|| panic!("missing table {sig}"))
            .1
    }

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
                &resource_qword_memory(machine::PCIE_MMIO_64.base, machine::PCIE_MMIO_64.size),
            ),
            "DSDT must describe the PCI 64-bit MMIO aperture"
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
}
