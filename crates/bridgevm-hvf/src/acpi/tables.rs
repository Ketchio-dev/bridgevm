//! The ACPI blobs for fw_cfg: the table writer, the loader script, `build_acpi`,
//! and every table except the DSDT (RSDP, XSDT, FADT, MADT, PPTT, GTDT, MCFG,
//! SPCR, DBG2, TPM2).

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

/// fw_cfg file carrying the concatenated ACPI tables.
pub const ACPI_TABLE_FILE: &str = "etc/acpi/tables";
/// fw_cfg file carrying the RSDP.
pub const ACPI_RSDP_FILE: &str = "etc/acpi/rsdp";
/// fw_cfg file carrying loader/linker commands.
pub const ACPI_LOADER_FILE: &str = "etc/table-loader";
/// fw_cfg file allocated as the TPM 2.0 measured-boot log.
pub const ACPI_TPM_LOG_FILE: &str = "etc/tpm/log";
pub const TPM_LOG_AREA_MINIMUM_SIZE: usize = 64 * 1024;

/// The three blobs the firmware fetches from `fw_cfg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpiBlobs {
    /// `etc/acpi/rsdp` — the Root System Description Pointer (36 bytes, v2).
    ///
    /// Checksum bytes are zero here; the firmware computes final checksums after
    /// applying the `bios-linker-loader` relocation contract.
    pub rsdp: Vec<u8>,
    /// `etc/acpi/tables` — XSDT, FADT, DSDT, MADT, PPTT, GTDT, MCFG, SPCR and
    /// DBG2, concatenated in the order their physical addresses are laid out.
    ///
    /// Checksum bytes are zero here; the firmware computes final checksums after
    /// applying the `bios-linker-loader` relocation contract.
    pub tables: Vec<u8>,
    /// `etc/table-loader` — loader commands that allocate the two files,
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
        assert_eq!(bytes.len(), ACPI_HEADER_LEN);
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
        self.bytes.extend(std::iter::repeat_n(0u8, n));
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
    /// The Mac's AC adapter (`ADP0`) and battery (`BAT0`), with their GED
    /// events. Off leaves the DSDT exactly as without the feature.
    pub host_power_present: bool,
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
    assert_eq!(fadt.len() as u64, fadt_len());

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
    assert_eq!(xsdt.len() as u64, xsdt_len);

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

    // Allocate every file before emitting pointer and checksum commands.
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

    assert_eq!(loader.len() % LOADER_ENTRY_LEN, 0);
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
    assert_eq!(r.len(), 36);

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

/// TPM2 ACPI revision-4 client table for a FIFO/TIS device, following the TCG
/// ACPI layout. The loader relocates LASA (offset 68) to the
/// separately allocated `etc/tpm/log` buffer.
pub(crate) fn build_tpm2() -> Vec<u8> {
    let mut t = Table::new(b"TPM2", 4);
    t.u16(0); // Platform Class = client
    t.u16(0); // Reserved
    t.u64(0); // Control Area address: unused for FIFO/TIS
    t.u32(6); // Start Method = MMIO
    t.pad(12); // Platform-specific start-method parameters
    t.u32(TPM_LOG_AREA_MINIMUM_SIZE as u32); // LAML
    t.u64(0); // LASA, relocated by the fw_cfg table loader
    let table = t.finish();
    assert_eq!(table.len(), 76);
    table
}

/// MADT (Multiple APIC Description Table) for GICv3: one GICC per CPU, a single
/// GICD, and a GICR covering the whole redistributor window.
pub(crate) fn build_madt(cpu_count: u64) -> Vec<u8> {
    let mut t = Table::new(b"APIC", 4); // MADT signature is "APIC"
    t.u32(0); // Local Interrupt Controller Address (unused on GICv3)
    t.u32(0); // Flags (no PC-AT 8259)

    // GIC Distributor (type 0x0C) — exactly one, before per-CPU GICC entries.
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
pub(crate) fn redistributor_base(cpu: u64) -> u64 {
    machine::GIC_REDIST.base + cpu * machine::GICV3_REDIST_STRIDE
}

/// Linear MPIDR affinity for `cpu` (Aff0 = 0..15, Aff1 = group of 16).
pub(crate) fn mpidr_for(cpu: u64) -> u64 {
    machine::cpu_mpidr(cpu)
}

pub(crate) const PPTT_NODE_PROCESSOR: u8 = 0;
pub(crate) const PPTT_PROCESSOR_PHYSICAL_PACKAGE: u32 = 1 << 0;
pub(crate) const PPTT_PROCESSOR_ACPI_ID_VALID: u32 = 1 << 1;
pub(crate) const PPTT_PROCESSOR_LEAF: u32 = 1 << 3;
pub(crate) const PPTT_PROCESSOR_IDENTICAL: u32 = 1 << 4;

/// Append an ACPI PPTT processor hierarchy node (type 0). Offsets stored in
/// `parent` and `private_resources` are relative to the start of the PPTT table.
pub(crate) fn append_pptt_processor_node(
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

/// PPTT homogeneous topology: one root package node, one socket node, and one
/// leaf processor node per ACPI Processor UID.
pub(crate) fn build_pptt(cpu_count: u64) -> Vec<u8> {
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
pub(crate) fn build_gtdt() -> Vec<u8> {
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
pub(crate) fn ppi_to_gsiv(ppi: u32) -> u32 {
    ppi + 16
}

/// MCFG (PCI memory-mapped configuration space) describing the ECAM window for
/// PCI segment 0, buses 0..=255.
pub(crate) fn build_mcfg() -> Vec<u8> {
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
pub(crate) fn build_spcr() -> Vec<u8> {
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
pub(crate) fn build_dbg2() -> Vec<u8> {
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
