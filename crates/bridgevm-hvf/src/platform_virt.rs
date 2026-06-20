//! `VirtPlatform` — the assembled Path A "QEMU virt" platform.
//!
//! This ties the three Path A bricks together into the object a live HVF run
//! loop drives:
//!
//! - [`crate::machine`] — the device memory map + IRQ map (single source of truth).
//! - [`crate::fwcfg`] — the `fw_cfg` keystone, populated with the guest device
//!   tree, ACPI tables, SMBIOS and boot order.
//! - [`crate::dtb`] — the QEMU-`virt`-shaped device tree handed to firmware.
//!
//! The live wiring is small and lives at the data-abort (MMIO) exit of
//! `hv_vcpu_run`: on a guest MMIO fault the run loop calls [`VirtPlatform::on_mmio`]
//! with the fault address, access, and a [`GuestMemoryMut`] view of guest RAM, and
//! applies the [`MmioOutcome`]. Everything in this module is host-only and
//! unit-testable; only the `hv_vcpu_run` call itself needs an entitled,
//! code-signed Apple Silicon host (the step-6 Linux ACPI-only bring-up in
//! `docs/hvf-windows-engine-strategy.md`).

use crate::acpi::{build_acpi, ACPI_LOADER_FILE, ACPI_RSDP_FILE, ACPI_TABLE_FILE};
use crate::dtb::{build_virt_fdt, VirtFdtConfig};
use crate::fwcfg::{FwCfg, GuestMemoryMut};
use crate::machine::{self, Region};
use crate::pcie::PcieEcam;
use crate::pflash::P30NorFlash;
use crate::pl011::Pl011;
use crate::pl031::Pl031;

/// A guest MMIO access as decoded from an HVF data-abort exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmioOp {
    Read { size: u8 },
    Write { size: u8, value: u64 },
}

/// Result of dispatching a guest MMIO access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MmioOutcome {
    /// A read completed; this value is written back to the faulting register.
    ReadValue(u64),
    /// A write was accepted by a device.
    WriteAck,
    /// The address belongs to a modelled device that is not implemented yet.
    /// Carries the device name so bring-up traces are precise rather than a
    /// generic "unhandled MMIO".
    KnownUnimplemented(&'static str),
    /// The address belongs to no device in the machine map.
    Unmapped,
}

/// Where the firmware, device tree and RAM live in the guest address space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestMemoryLayout {
    /// pflash bank 0 — firmware code (read-only).
    pub flash_code: Region,
    /// pflash bank 1 — UEFI variable store (writable).
    pub flash_vars: Region,
    /// System RAM.
    pub ram: Region,
    /// Address the flattened device tree is loaded at (inside RAM).
    pub dtb_load: u64,
}

/// The assembled Path A platform.
#[derive(Debug)]
pub struct VirtPlatform {
    cfg: VirtFdtConfig,
    fw_cfg: FwCfg,
    uart: Pl011,
    rtc: Pl031,
    pcie: PcieEcam,
    flash_vars: P30NorFlash,
    dtb: Vec<u8>,
}

impl VirtPlatform {
    /// Build the platform: generate the device tree from the machine map and
    /// stand up `fw_cfg` with its standard control entries. ACPI/SMBIOS blobs are
    /// attached separately via [`Self::set_acpi_tables`] / [`Self::set_smbios`].
    pub fn new(cfg: VirtFdtConfig) -> Self {
        let dtb = build_virt_fdt(&cfg);
        let mut fw_cfg = FwCfg::new();
        // Minimal real control entries the firmware/OS consult.
        fw_cfg.add_file("bootorder", Vec::new());
        // `etc/system-states` advertises which ACPI S-states are enabled; the
        // firmware may write it back, so it is writable. 6 bytes: S3, S4, ... .
        fw_cfg.add_writable_file("etc/system-states", vec![0u8; 6]);
        let acpi = build_acpi(cfg.cpu_count);
        fw_cfg.add_file(ACPI_RSDP_FILE, acpi.rsdp);
        fw_cfg.add_file(ACPI_TABLE_FILE, acpi.tables);
        fw_cfg.add_file(ACPI_LOADER_FILE, acpi.loader);
        Self {
            cfg,
            fw_cfg,
            uart: Pl011::new(),
            rtc: Pl031::new(),
            pcie: PcieEcam::new(),
            flash_vars: P30NorFlash::new(
                machine::FLASH_VARS.base,
                machine::FLASH_VARS.size as usize,
                0x40000,
            ),
            dtb,
        }
    }

    /// Load the writable pflash bank backing bytes. Live HVF code leaves the vars
    /// bank unmapped so NOR command/status reads and writes trap here instead of
    /// being treated as plain RAM stores.
    pub fn load_flash_vars(&mut self, data: &[u8]) {
        self.flash_vars.load(data);
    }

    /// Register the guest ACPI tables (`etc/acpi/rsdp`, `etc/acpi/tables`,
    /// `etc/table-loader`) the firmware installs. On Path A these come from the
    /// QEMU-style table generator; until that lands this lets callers attach
    /// known-good bytes (e.g. captured from the QEMU oracle) so the rest of the
    /// pipeline can be exercised end to end.
    pub fn set_acpi_tables(&mut self, rsdp: Vec<u8>, tables: Vec<u8>, loader: Vec<u8>) {
        self.fw_cfg.add_file(ACPI_RSDP_FILE, rsdp);
        self.fw_cfg.add_file(ACPI_TABLE_FILE, tables);
        self.fw_cfg.add_file(ACPI_LOADER_FILE, loader);
    }

    /// Register the SMBIOS entry point + tables (`etc/smbios/smbios-anchor`,
    /// `etc/smbios/smbios-tables`).
    pub fn set_smbios(&mut self, anchor: Vec<u8>, tables: Vec<u8>) {
        self.fw_cfg.add_file("etc/smbios/smbios-anchor", anchor);
        self.fw_cfg.add_file("etc/smbios/smbios-tables", tables);
    }

    /// The generated device tree blob (DTB magic `0xd00dfeed`).
    pub fn dtb(&self) -> &[u8] {
        &self.dtb
    }

    /// The guest memory layout. The DTB is placed at the base of RAM, where the
    /// firmware looks for it; the kernel/initrd are loaded above it.
    pub fn memory_layout(&self) -> GuestMemoryLayout {
        GuestMemoryLayout {
            flash_code: machine::FLASH_CODE,
            flash_vars: machine::FLASH_VARS,
            ram: Region::new(machine::RAM_BASE, self.cfg.ram_size),
            dtb_load: machine::RAM_BASE,
        }
    }

    /// Dispatch a guest MMIO access. This is the single entry point the live HVF
    /// run loop calls from its data-abort exit handler.
    pub fn on_mmio(&mut self, gpa: u64, op: MmioOp, mem: &mut dyn GuestMemoryMut) -> MmioOutcome {
        let Some(device) = machine::device_at(gpa) else {
            return MmioOutcome::Unmapped;
        };
        match device {
            "fw-cfg" => self.fw_cfg_access(gpa - machine::FW_CFG.base, op, mem),
            "uart" => self.uart_access(gpa - machine::UART.base, op),
            "rtc" => self.rtc_access(gpa - machine::RTC.base, op),
            "pcie-ecam" => self.pcie_access(gpa - machine::PCIE_ECAM.base, op),
            "virtio-mmio" => self.virtio_mmio_access(gpa - machine::VIRTIO_MMIO.base, op),
            "flash-vars" => self.flash_vars.access(gpa, op),
            // Modelled in the machine map but no device behaviour yet — surfaced
            // precisely so bring-up traces show the next thing to implement.
            other => MmioOutcome::KnownUnimplemented(other),
        }
    }

    /// Empty virtio-mmio transport slot. Advertise a valid v2 register block with
    /// DeviceID 0 so the firmware sees "valid transport, no device" and skips it
    /// silently — matching QEMU's empty slots. Returning 0 (no magic) instead made
    /// VirtioMmioInit fail with EFI_UNSUPPORTED and log 32 errors per boot.
    fn virtio_mmio_access(&self, slot_offset: u64, op: MmioOp) -> MmioOutcome {
        match op {
            MmioOp::Read { .. } => {
                let value = match slot_offset % machine::VIRTIO_MMIO_SLOT_SIZE {
                    0x00 => 0x7472_6976, // MagicValue, "virt"
                    0x04 => 0x2,         // Version: virtio-mmio 1.0+
                    0x08 => 0x0,         // DeviceID: 0 = no device present
                    0x0c => 0x554d_4551, // VendorID, "QEMU"
                    _ => 0,
                };
                MmioOutcome::ReadValue(value)
            }
            MmioOp::Write { .. } => MmioOutcome::WriteAck,
        }
    }

    /// PCIe ECAM config-space access: a real host bridge at 00:00.0, all-ones
    /// (no device) elsewhere. Replaces the earlier blanket all-ones stub.
    fn pcie_access(&mut self, ecam_offset: u64, op: MmioOp) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.pcie.cfg_read(ecam_offset, size)),
            MmioOp::Write { size, value } => {
                self.pcie.cfg_write(ecam_offset, size, value);
                MmioOutcome::WriteAck
            }
        }
    }

    fn uart_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.uart.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.uart.mmio_write(offset, size, value);
                MmioOutcome::WriteAck
            }
        }
    }

    fn rtc_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.rtc.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.rtc.mmio_write(offset, size, value);
                MmioOutcome::WriteAck
            }
        }
    }

    /// Bytes the guest/firmware has written to the UART so far.
    pub fn uart_output(&self) -> &[u8] {
        self.uart.output()
    }

    fn fw_cfg_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.fw_cfg.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.fw_cfg.mmio_write(offset, size, value, mem);
                MmioOutcome::WriteAck
            }
        }
    }
}

/// A flat span of guest RAM implementing [`GuestMemoryMut`]. In live use the run
/// loop supplies a view over the HVF-mapped guest memory instead; this is the
/// host-side stand-in used for tests and offline pipeline exercises.
#[derive(Debug)]
pub struct FlatGuestRam {
    base: u64,
    bytes: Vec<u8>,
}

impl FlatGuestRam {
    pub fn new(base: u64, len: usize) -> Self {
        Self {
            base,
            bytes: vec![0u8; len],
        }
    }
    fn offset(&self, gpa: u64) -> Option<usize> {
        gpa.checked_sub(self.base).map(|o| o as usize)
    }
}

impl GuestMemoryMut for FlatGuestRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(start) = self.offset(gpa) else {
            return false;
        };
        let end = start + data.len();
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[start..end].copy_from_slice(data);
        true
    }
    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = self.offset(gpa)?;
        let end = start + len;
        if end > self.bytes.len() {
            return None;
        }
        Some(self.bytes[start..end].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fwcfg::{DMA_CTL_READ, DMA_CTL_SELECT, KEY_SIGNATURE};
    use crate::machine;

    const REG_DATA: u64 = 0x0;
    const REG_SELECTOR: u64 = 0x8;
    const REG_DMA: u64 = 0x10;

    fn platform() -> VirtPlatform {
        VirtPlatform::new(VirtFdtConfig::default())
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
        // ...then a 4-byte data read returns "QEMU" big-endian.
        let v = p.on_mmio(
            machine::FW_CFG.base + REG_DATA,
            MmioOp::Read { size: 4 },
            &mut mem,
        );
        assert_eq!(v, MmioOutcome::ReadValue(0x5145_4d55));
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
        // An empty slot (device 1, ECAM offset dev<<15) reads all-ones (no device).
        assert_eq!(
            p.on_mmio(
                machine::PCIE_ECAM.base + (1 << 15),
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(0xFFFF_FFFF)
        );
    }

    #[test]
    fn uart_writes_are_captured_via_mmio() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        for b in b"HI\n" {
            assert_eq!(
                p.on_mmio(
                    machine::UART.base,
                    MmioOp::Write {
                        size: 1,
                        value: u64::from(*b)
                    },
                    &mut mem
                ),
                MmioOutcome::WriteAck
            );
        }
        assert_eq!(p.uart_output(), b"HI\n");
        // UARTFR (offset 0x18) reports transmit-ready (TXFE set).
        assert!(matches!(
            p.on_mmio(machine::UART.base + 0x18, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(v) if v & (1 << 7) != 0
        ));
    }

    #[test]
    fn rtc_data_and_id_registers_are_modelled() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        assert_eq!(
            p.on_mmio(
                machine::RTC.base + 0xfe0,
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(0x31)
        );
        match p.on_mmio(machine::RTC.base, MmioOp::Read { size: 4 }, &mut mem) {
            MmioOutcome::ReadValue(value) => assert!(value > 1_600_000_000),
            other => panic!("unexpected RTC read outcome: {other:?}"),
        }
        assert_eq!(
            p.on_mmio(
                machine::RTC.base + 0x008,
                MmioOp::Write {
                    size: 4,
                    value: 0x2026_0619,
                },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(machine::RTC.base, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x2026_0619)
        );
    }

    #[test]
    fn flash_vars_routes_nor_status_protocol() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        p.load_flash_vars(&[0x78, 0x56, 0x34, 0x12]);
        assert_eq!(
            p.on_mmio(machine::FLASH_VARS.base, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x1234_5678)
        );
        assert_eq!(
            p.on_mmio(
                machine::FLASH_VARS.base,
                MmioOp::Write {
                    size: 4,
                    value: 0x0070_0070,
                },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(machine::FLASH_VARS.base, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x0080_0080)
        );
    }

    #[test]
    fn generated_acpi_tables_are_registered_by_default() {
        let mut p = platform();
        p.fw_cfg.select(crate::fwcfg::KEY_FILE_DIR);
        let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
        let blob = String::from_utf8_lossy(&dir);
        for name in [ACPI_RSDP_FILE, ACPI_TABLE_FILE, ACPI_LOADER_FILE] {
            assert!(blob.contains(name), "default fw_cfg dir missing {name}");
        }
    }

    #[test]
    fn acpi_and_smbios_tables_register_into_fw_cfg() {
        let mut p = platform();
        p.set_acpi_tables(vec![0xAA; 36], vec![0xBB; 100], vec![0xCC; 40]);
        p.set_smbios(vec![0x5F; 24], vec![0x01; 80]);
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        // Read the FILE_DIR through fw_cfg and confirm the names are present.
        p.fw_cfg.select(crate::fwcfg::KEY_FILE_DIR);
        let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
        let blob = String::from_utf8_lossy(&dir);
        for name in [
            "etc/acpi/rsdp",
            "etc/acpi/tables",
            "etc/table-loader",
            "etc/smbios/smbios-anchor",
            "bootorder",
        ] {
            assert!(blob.contains(name), "fw_cfg dir missing {name}");
        }
        // Suppress unused-variable warning for `mem` in this assertion-only test.
        let _ = &mut mem;
    }
}
