//! Split out of acpi.rs to keep files under 850 lines.

use super::*;

use crate::machine;

/// Length of an ACPI standard description-header (`signature` .. `creator_revision`).
pub(crate) const ACPI_HEADER_LEN: usize = 36;

/// OEM identity stamped into every table header (6 + 8 + 4 bytes).
pub(crate) const OEM_ID: &[u8; 6] = b"BRDGVM";
pub(crate) const OEM_TABLE_ID: &[u8; 8] = b"BVMVIRT ";
pub(crate) const OEM_REVISION: u32 = 1;
pub(crate) const CREATOR_ID: &[u8; 4] = b"BVM ";
pub(crate) const CREATOR_REVISION: u32 = 1;

/// QEMU fw_cfg file carrying the concatenated ACPI tables.
pub const ACPI_TABLE_FILE: &str = "etc/acpi/tables";
/// QEMU fw_cfg file carrying the RSDP.
pub const ACPI_RSDP_FILE: &str = "etc/acpi/rsdp";
/// QEMU fw_cfg file carrying loader/linker commands.
pub const ACPI_LOADER_FILE: &str = "etc/table-loader";
/// QEMU-compatible fw_cfg file allocated as the TPM 2.0 measured-boot log.
pub const ACPI_TPM_LOG_FILE: &str = "etc/tpm/log";
pub const TPM_LOG_AREA_MINIMUM_SIZE: usize = 64 * 1024;

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
    /// Zero-initialized firmware-writable measured-boot log allocation. It is
    /// present only when the TPM device is advertised.
    pub tpm_log: Option<Vec<u8>>,
}

/// One-byte ACPI checksum: the value that makes the sum of every byte in
/// `bytes` (including the checksum byte itself) wrap to zero mod 256.
pub(crate) fn checksum(bytes: &[u8]) -> u8 {
    let sum = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    sum.wrapping_neg()
}

/// A description table under construction. Reserves the 36-byte header up front
/// and exposes little-endian append helpers; [`Self::finish`] back-patches the
/// length and checksum so the finished blob sums to zero.
pub(crate) struct Table {
    pub(crate) bytes: Vec<u8>,
}

impl Table {
    /// Begin a table with the given 4-byte signature and revision, reserving a
    /// zeroed standard header.
    pub(crate) fn new(signature: &[u8; 4], revision: u8) -> Self {
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

    pub(crate) fn u8(&mut self, v: u8) {
        self.bytes.push(v);
    }
    pub(crate) fn u16(&mut self, v: u16) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }
    pub(crate) fn u32(&mut self, v: u32) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }
    pub(crate) fn u64(&mut self, v: u64) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }
    /// Append `n` zero bytes (reserved fields).
    pub(crate) fn pad(&mut self, n: usize) {
        self.bytes.extend(std::iter::repeat(0u8).take(n));
    }

    /// A 12-byte ACPI Generic Address Structure (GAS) with an explicit ACPI
    /// access-size encoding (1=byte, 2=word,
    /// 3=dword, 4=qword). SPCR consumers warn if this is left undefined.
    pub(crate) fn gas_memory_with_access_size(
        &mut self,
        address: u64,
        bit_width: u8,
        access_size: u8,
    ) {
        self.u8(0x00); // AddressSpaceId = SystemMemory
        self.u8(bit_width);
        self.u8(0x00); // BitOffset
        self.u8(access_size);
        self.u64(address);
    }

    /// A null Generic Address Structure (all fields zero) — used where the spec
    /// allows "not present".
    pub(crate) fn gas_null(&mut self) {
        self.pad(12);
    }

    /// Back-patch length + checksum and return the finished bytes.
    pub(crate) fn finish(mut self) -> Vec<u8> {
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
pub(crate) const FADT_FLAG_HW_REDUCED_ACPI: u32 = 1 << 20;

/// ARM boot architecture flags (FADT offset 129).
pub(crate) const FADT_ARM_BOOT_PSCI_COMPLIANT: u16 = 1 << 0;
/// PSCI is invoked via `HVC` rather than `SMC`.
pub(crate) const FADT_ARM_BOOT_PSCI_USE_HVC: u16 = 1 << 1;

// ---- Builder ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AcpiDeviceConfig {
    pub tpm_tis_present: bool,
}

/// Build the ACPI blobs for the default device set.
pub fn build_acpi(cpu_count: u64) -> AcpiBlobs {
    build_acpi_with_devices(cpu_count, AcpiDeviceConfig::default())
}

/// Build the `etc/acpi/rsdp` and `etc/acpi/tables` blobs for a `cpu_count`-CPU
/// guest and an explicit optional-device set. Panics if `cpu_count` exceeds
/// what the GICv3 redistributor window can host.
pub fn build_acpi_with_devices(cpu_count: u64, devices: AcpiDeviceConfig) -> AcpiBlobs {
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

    let dsdt = build_dsdt(cpu_count, devices);
    let madt = build_madt(cpu_count);
    let pptt = build_pptt(cpu_count);
    let gtdt = build_gtdt();
    let mcfg = build_mcfg();
    let spcr = build_spcr();
    let dbg2 = build_dbg2();
    let tpm2 = devices.tpm_tis_present.then(build_tpm2);

    // The XSDT references FADT/MADT/PPTT/GTDT/MCFG/SPCR/DBG2. The FADT
    // references the DSDT. Compute offsets in concatenation order: XSDT first,
    // then the rest. (Order within the blob is a free choice; we keep XSDT first
    // so its address is easy to reason about, then DSDT, then the XSDT-listed
    // tables.)
    let xsdt_entry_count = 7 + usize::from(tpm2.is_some());
    let xsdt_len = xsdt_len_for(xsdt_entry_count);
    let off_xsdt = 0u64;
    let off_dsdt = off_xsdt + xsdt_len;
    let off_fadt = off_dsdt + dsdt.len() as u64;
    let off_madt = off_fadt + fadt_len();
    let off_pptt = off_madt + madt.len() as u64;
    let off_gtdt = off_pptt + pptt.len() as u64;
    let off_mcfg = off_gtdt + gtdt.len() as u64;
    let off_spcr = off_mcfg + mcfg.len() as u64;
    let off_dbg2 = off_spcr + spcr.len() as u64;
    let off_tpm2 = tpm2.as_ref().map(|_| off_dbg2 + dbg2.len() as u64);

    let fadt = build_fadt(TABLES_BASE + off_dsdt);
    debug_assert_eq!(fadt.len() as u64, fadt_len());

    let mut xsdt_entries = vec![
        TABLES_BASE + off_fadt,
        TABLES_BASE + off_madt,
        TABLES_BASE + off_pptt,
        TABLES_BASE + off_gtdt,
        TABLES_BASE + off_mcfg,
        TABLES_BASE + off_spcr,
        TABLES_BASE + off_dbg2,
    ];
    if let Some(offset) = off_tpm2 {
        xsdt_entries.push(TABLES_BASE + offset);
    }
    let xsdt = build_xsdt(&xsdt_entries);
    debug_assert_eq!(xsdt.len() as u64, xsdt_len);

    let mut table_spans = vec![
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
    if let (Some(offset), Some(table)) = (off_tpm2, tpm2.as_ref()) {
        table_spans.push(TableSpan::new(offset, table.len() as u64));
    }

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
    if let Some(table) = tpm2.as_ref() {
        tables.extend_from_slice(table);
    }

    let mut rsdp = build_rsdp(TABLES_BASE + off_xsdt);
    let loader = build_table_loader(
        &mut rsdp,
        &mut tables,
        LoaderLayout {
            xsdt: off_xsdt,
            fadt: off_fadt,
            table_spans: &table_spans,
            xsdt_entries: &xsdt_entries,
            tpm2_log_area_start: off_tpm2.map(|offset| offset + TPM2_LOG_AREA_START_OFFSET),
        },
    );

    AcpiBlobs {
        rsdp,
        tables,
        loader,
        tpm_log: devices
            .tpm_tis_present
            .then(|| vec![0; TPM_LOG_AREA_MINIMUM_SIZE]),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TableSpan {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

impl TableSpan {
    pub(crate) fn new(start: u64, len: u64) -> Self {
        Self {
            start: u32::try_from(start).expect("ACPI table offset exceeds 4 GiB"),
            len: u32::try_from(len).expect("ACPI table length exceeds 4 GiB"),
        }
    }
}

pub(crate) struct LoaderLayout<'a> {
    pub(crate) xsdt: u64,
    pub(crate) fadt: u64,
    pub(crate) table_spans: &'a [TableSpan],
    pub(crate) xsdt_entries: &'a [u64],
    pub(crate) tpm2_log_area_start: Option<u64>,
}

pub(crate) const LOADER_ENTRY_LEN: usize = 128;
pub(crate) const LOADER_PAYLOAD_LEN: usize = 124;
pub(crate) const LOADER_FILE_NAME_LEN: usize = 56;

pub(crate) const LOADER_CMD_ALLOCATE: u32 = 1;
pub(crate) const LOADER_CMD_ADD_POINTER: u32 = 2;
pub(crate) const LOADER_CMD_ADD_CHECKSUM: u32 = 3;

pub(crate) const LOADER_ZONE_HIGH: u8 = 1;
pub(crate) const LOADER_ZONE_FSEG: u8 = 2;

pub(crate) const TABLE_ALLOC_ALIGN: u32 = 64;
pub(crate) const RSDP_ALLOC_ALIGN: u32 = 16;
pub(crate) const ACPI_CHECKSUM_OFFSET: u32 = 9;
pub(crate) const RSDP_V1_CHECKSUM_OFFSET: u32 = 8;
pub(crate) const RSDP_EXT_CHECKSUM_OFFSET: u32 = 32;
pub(crate) const RSDP_XSDT_OFFSET: u32 = 24;
pub(crate) const FADT_X_DSDT_OFFSET: u32 = 140;
pub(crate) const TPM2_LOG_AREA_START_OFFSET: u64 = 68;

pub(crate) fn build_table_loader(
    rsdp: &mut [u8],
    tables: &mut [u8],
    layout: LoaderLayout<'_>,
) -> Vec<u8> {
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
    if layout.tpm2_log_area_start.is_some() {
        loader.extend(alloc_entry(ACPI_TPM_LOG_FILE, 1, LOADER_ZONE_HIGH));
    }

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
    if let Some(offset) = layout.tpm2_log_area_start {
        loader.extend(add_pointer_entry(
            ACPI_TABLE_FILE,
            u32_checked(offset),
            8,
            ACPI_TPM_LOG_FILE,
        ));
    }

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

pub(crate) fn u32_checked(v: u64) -> u32 {
    u32::try_from(v).expect("ACPI loader offset exceeds 4 GiB")
}

pub(crate) fn loader_entry(
    command: u32,
    payload: [u8; LOADER_PAYLOAD_LEN],
) -> [u8; LOADER_ENTRY_LEN] {
    let mut entry = [0u8; LOADER_ENTRY_LEN];
    entry[..4].copy_from_slice(&command.to_le_bytes());
    entry[4..].copy_from_slice(&payload);
    entry
}

pub(crate) fn write_loader_name(dst: &mut [u8], name: &str) {
    assert!(
        name.len() < LOADER_FILE_NAME_LEN,
        "loader file name must be < {LOADER_FILE_NAME_LEN} bytes: {name:?}",
    );
    dst[..name.len()].copy_from_slice(name.as_bytes());
}

pub(crate) fn alloc_entry(file: &str, align: u32, zone: u8) -> [u8; LOADER_ENTRY_LEN] {
    let mut payload = [0u8; LOADER_PAYLOAD_LEN];
    write_loader_name(&mut payload[..LOADER_FILE_NAME_LEN], file);
    payload[LOADER_FILE_NAME_LEN..LOADER_FILE_NAME_LEN + 4].copy_from_slice(&align.to_le_bytes());
    payload[LOADER_FILE_NAME_LEN + 4] = zone;
    loader_entry(LOADER_CMD_ALLOCATE, payload)
}

pub(crate) fn add_pointer_entry(
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

pub(crate) fn add_checksum_entry(
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
pub(crate) fn xsdt_len_for(entries: usize) -> u64 {
    (ACPI_HEADER_LEN + entries * 8) as u64
}

/// RSDP (Root System Description Pointer), ACPI 2.0+ (revision 2). 36 bytes with
/// two checksums: the 20-byte v1 checksum and the full-structure extended one.
pub(crate) fn build_rsdp(xsdt_address: u64) -> Vec<u8> {
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
pub(crate) fn build_xsdt(entries: &[u64]) -> Vec<u8> {
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
pub(crate) fn build_fadt(dsdt_address: u64) -> Vec<u8> {
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
pub(crate) fn fadt_len() -> u64 {
    // Built once to keep the layout single-sourced; cheap enough at startup.
    build_fadt(0).len() as u64
}

// ---- AML helpers ------------------------------------------------------------

pub(crate) const AML_ZERO_OP: u8 = 0x00;
pub(crate) const AML_ONE_OP: u8 = 0x01;
pub(crate) const AML_BYTE_PREFIX: u8 = 0x0A;
pub(crate) const AML_DWORD_PREFIX: u8 = 0x0C;
pub(crate) const AML_STRING_PREFIX: u8 = 0x0D;
pub(crate) const AML_NAME_OP: u8 = 0x08;
pub(crate) const AML_SCOPE_OP: u8 = 0x10;
pub(crate) const AML_BUFFER_OP: u8 = 0x11;
pub(crate) const AML_PACKAGE_OP: u8 = 0x12;
pub(crate) const AML_METHOD_OP: u8 = 0x14;
pub(crate) const AML_EXT_OP: u8 = 0x5B;
pub(crate) const AML_DEVICE_OP: u8 = 0x82;
pub(crate) const AML_OPERATION_REGION_OP: u8 = 0x80;
pub(crate) const AML_FIELD_OP: u8 = 0x81;
pub(crate) const AML_LOCAL0_OP: u8 = 0x60;
pub(crate) const AML_ARG0_OP: u8 = 0x68;
pub(crate) const AML_STORE_OP: u8 = 0x70;
pub(crate) const AML_ADD_OP: u8 = 0x72;
pub(crate) const AML_AND_OP: u8 = 0x7B;
pub(crate) const AML_OR_OP: u8 = 0x7D;
pub(crate) const AML_DEREF_OF_OP: u8 = 0x83;
pub(crate) const AML_INDEX_OP: u8 = 0x88;
pub(crate) const AML_CREATE_DWORD_FIELD_OP: u8 = 0x8A;
pub(crate) const AML_LNOT_OP: u8 = 0x92;
pub(crate) const AML_LEQUAL_OP: u8 = 0x93;
pub(crate) const AML_IF_OP: u8 = 0xA0;
pub(crate) const AML_ELSE_OP: u8 = 0xA1;
pub(crate) const AML_RETURN_OP: u8 = 0xA4;

pub(crate) const EISA_PNP0A08: [u8; 4] = [0x41, 0xD0, 0x0A, 0x08];
pub(crate) const EISA_PNP0A03: [u8; 4] = [0x41, 0xD0, 0x0A, 0x03];
pub(crate) const EISA_PNP0C02: [u8; 4] = [0x41, 0xD0, 0x0C, 0x02];
pub(crate) const EISA_PNP0C0C: [u8; 4] = [0x41, 0xD0, 0x0C, 0x0C];
pub(crate) const PCI_HOST_BRIDGE_OSC_UUID: [u8; 16] = [
    0x5B, 0x4D, 0xDB, 0x33, 0xF7, 0x1F, 0x1C, 0x40, 0x96, 0x57, 0x74, 0x41, 0xC0, 0x3D, 0xD7, 0x66,
];
pub(crate) const TPM_PPI_DSM_UUID: [u8; 16] = [
    0xA6, 0xFA, 0xDD, 0x3D, 0x1B, 0x36, 0xB4, 0x4E, 0xA4, 0x24, 0x8D, 0x10, 0x08, 0x9D, 0x16, 0x53,
];
pub(crate) const TPM_RESET_ATTACK_DSM_UUID: [u8; 16] = [
    0xED, 0x54, 0x60, 0x37, 0x13, 0xCC, 0x75, 0x46, 0x90, 0x1C, 0x47, 0x56, 0xD7, 0xF2, 0xD4, 0x5D,
];

pub(crate) fn aml_pkg_length(payload_len: usize) -> Vec<u8> {
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

pub(crate) fn aml_name_string(name: &[u8; 4], value: &str) -> Vec<u8> {
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

pub(crate) fn aml_string(value: &str) -> Vec<u8> {
    assert!(
        !value.as_bytes().contains(&0),
        "AML strings are NUL-terminated"
    );
    let mut out = vec![AML_STRING_PREFIX];
    out.extend_from_slice(value.as_bytes());
    out.push(0);
    out
}

pub(crate) fn aml_name_eisa(name: &[u8; 4], encoded: [u8; 4]) -> Vec<u8> {
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.push(AML_DWORD_PREFIX);
    out.extend_from_slice(&encoded);
    out
}

pub(crate) fn aml_name_simple(name: &[u8; 4], op: u8) -> Vec<u8> {
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.push(op);
    out
}

pub(crate) fn aml_name_byte(name: &[u8; 4], value: u8) -> Vec<u8> {
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.extend(aml_byte(value));
    out
}

pub(crate) fn aml_name_buffer(name: &[u8; 4], bytes: &[u8]) -> Vec<u8> {
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

pub(crate) fn aml_buffer(bytes: &[u8]) -> Vec<u8> {
    assert!(bytes.len() <= u8::MAX as usize, "small AML buffer expected");
    let mut out = vec![AML_BUFFER_OP];
    out.extend(aml_pkg_length(2 + bytes.len()));
    out.push(AML_BYTE_PREFIX);
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
    out
}

pub(crate) fn aml_package(elements: &[Vec<u8>]) -> Vec<u8> {
    assert!(
        elements.len() <= u8::MAX as usize,
        "small AML package expected"
    );
    let payload_len = 1 + elements.iter().map(Vec::len).sum::<usize>();
    let mut out = vec![AML_PACKAGE_OP];
    out.extend(aml_pkg_length(payload_len));
    out.push(elements.len() as u8);
    for element in elements {
        out.extend_from_slice(element);
    }
    out
}

pub(crate) fn aml_name_package(name: &[u8; 4], elements: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.extend(aml_package(elements));
    out
}

pub(crate) fn aml_name_ref(name: &[u8; 4]) -> Vec<u8> {
    name.to_vec()
}

pub(crate) fn aml_arg(n: u8) -> Vec<u8> {
    assert!(n <= 6, "AML has only Arg0..Arg6");
    vec![AML_ARG0_OP + n]
}

pub(crate) fn aml_local0() -> Vec<u8> {
    vec![AML_LOCAL0_OP]
}

pub(crate) fn aml_local(n: u8) -> Vec<u8> {
    assert!(n <= 7, "AML has only Local0..Local7");
    vec![AML_LOCAL0_OP + n]
}

pub(crate) fn aml_byte(value: u8) -> Vec<u8> {
    match value {
        0 => vec![AML_ZERO_OP],
        1 => vec![AML_ONE_OP],
        _ => vec![AML_BYTE_PREFIX, value],
    }
}

pub(crate) fn aml_dword(value: u32) -> Vec<u8> {
    let mut out = vec![AML_DWORD_PREFIX];
    out.extend_from_slice(&value.to_le_bytes());
    out
}

pub(crate) fn aml_uuid_buffer(bytes: &[u8; 16]) -> Vec<u8> {
    let mut out = vec![AML_BUFFER_OP];
    out.extend(aml_pkg_length(2 + bytes.len()));
    out.push(AML_BYTE_PREFIX);
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
    out
}

pub(crate) fn aml_create_dword_field(source: &[u8], byte_index: u8, name: &[u8; 4]) -> Vec<u8> {
    let mut out = vec![AML_CREATE_DWORD_FIELD_OP];
    out.extend_from_slice(source);
    out.extend(aml_byte(byte_index));
    out.extend_from_slice(name);
    out
}

pub(crate) fn aml_store(source: &[u8], target: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_STORE_OP];
    out.extend_from_slice(source);
    out.extend_from_slice(target);
    out
}

pub(crate) fn aml_index(source: &[u8], index: &[u8], target: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_INDEX_OP];
    out.extend_from_slice(source);
    out.extend_from_slice(index);
    out.extend_from_slice(target);
    out
}

pub(crate) fn aml_deref_of(reference: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_DEREF_OF_OP];
    out.extend_from_slice(reference);
    out
}

pub(crate) fn aml_call1(name: &[u8; 4], arg: &[u8]) -> Vec<u8> {
    let mut out = name.to_vec();
    out.extend_from_slice(arg);
    out
}

pub(crate) fn aml_binary_op(op: u8, left: &[u8], right: &[u8], target: &[u8]) -> Vec<u8> {
    let mut out = vec![op];
    out.extend_from_slice(left);
    out.extend_from_slice(right);
    out.extend_from_slice(target);
    out
}

pub(crate) fn aml_equal(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_LEQUAL_OP];
    out.extend_from_slice(left);
    out.extend_from_slice(right);
    out
}

pub(crate) fn aml_not_equal(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_LNOT_OP];
    out.extend(aml_equal(left, right));
    out
}

pub(crate) fn aml_if(predicate: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_IF_OP];
    out.extend(aml_pkg_length(predicate.len() + body.len()));
    out.extend_from_slice(predicate);
    out.extend_from_slice(body);
    out
}

pub(crate) fn aml_else(body: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_ELSE_OP];
    out.extend(aml_pkg_length(body.len()));
    out.extend_from_slice(body);
    out
}

pub(crate) fn aml_return(value: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_RETURN_OP];
    out.extend_from_slice(value);
    out
}

pub(crate) fn aml_operation_region(name: &[u8; 4], base: &[u8], length: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_EXT_OP, AML_OPERATION_REGION_OP];
    out.extend_from_slice(name);
    out.push(0x00); // SystemMemory
    out.extend_from_slice(base);
    out.extend_from_slice(length);
    out
}

pub(crate) fn aml_field(name: &[u8; 4], flags: u8, fields: &[(&[u8; 4], usize)]) -> Vec<u8> {
    let mut field_list = Vec::new();
    for (field_name, bit_length) in fields {
        field_list.extend_from_slice(*field_name);
        field_list.extend(aml_field_length(*bit_length));
    }
    let mut out = vec![AML_EXT_OP, AML_FIELD_OP];
    out.extend(aml_pkg_length(name.len() + 1 + field_list.len()));
    out.extend_from_slice(name);
    out.push(flags);
    out.extend(field_list);
    out
}
