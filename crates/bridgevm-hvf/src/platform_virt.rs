//! `VirtPlatform` — the assembled Path A "QEMU virt" platform.
//!
//! This ties the three Path A bricks together into the object a live HVF run
//! loop drives:
//!
//! - [`crate::machine`] — the device memory map + IRQ map (single source of truth).
//! - [`crate::fwcfg`] — the `fw_cfg` keystone, populated with the guest device
//!   tree, ACPI tables, SMBIOS and boot order.
//! - [`crate::dtb`] — the QEMU-`virt`-shaped device tree handed to firmware.
//! - [`crate::nvme`] — the first PCIe storage endpoint behind BAR0.
//!
//! The live wiring is small and lives at the data-abort (MMIO) exit of
//! `hv_vcpu_run`: on a guest MMIO fault the run loop calls [`VirtPlatform::on_mmio`]
//! with the fault address, access, and a [`GuestMemoryMut`] view of guest RAM, and
//! applies the [`MmioOutcome`]. Everything in this module is host-only and
//! unit-testable; only the `hv_vcpu_run` call itself needs an entitled,
//! code-signed Apple Silicon host (the step-6 Linux ACPI-only bring-up in
//! `docs/hvf-windows-engine-strategy.md`).

use std::{io, path::Path};

use crate::acpi::{build_acpi, ACPI_LOADER_FILE, ACPI_RSDP_FILE, ACPI_TABLE_FILE};
use crate::dtb::{build_virt_fdt, VirtFdtConfig};
use crate::fwcfg::{
    FwCfg, GuestMemoryMut, KEY_CMDLINE_DATA, KEY_CMDLINE_SIZE, KEY_INITRD_DATA, KEY_INITRD_SIZE,
    KEY_KERNEL_DATA, KEY_KERNEL_SIZE,
};
use crate::machine::{self, Region};
use crate::msix::MsixMessage;
use crate::nvme::{NvmeCommandTrace, NvmeCompletionEvent, NvmeController};
use crate::pcie::{PcieEcam, PcieMmioTarget, NVME_BDF, VIRTIO_BLK_BDF};
use crate::pflash::P30NorFlash;
use crate::pl011::Pl011;
use crate::pl031::Pl031;
use crate::smbios::{build_smbios, SMBIOS_ANCHOR_FILE, SMBIOS_TABLE_FILE};
use crate::virtio_blk::{
    VirtioMmioBlock, VirtioMmioBlockResult, VirtioMmioBlockStats, VirtioPciBlock, VirtioPciBlockOp,
    INSTALLER_ISO_SLOT,
};

const DEFAULT_NVME_DISK_BYTES: usize = 16 * 1024 * 1024;

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
    nvme: NvmeController,
    virtio_iso: Option<VirtioMmioBlock>,
    pci_boot_media: Option<VirtioPciBlock>,
    flash_vars: P30NorFlash,
    pending_msix: Vec<MsixMessage>,
    pending_spi_levels: Vec<(u32, bool)>,
    dtb: Vec<u8>,
}

impl VirtPlatform {
    /// Build the platform: generate the device tree from the machine map and
    /// stand up `fw_cfg` with its standard control entries and generated ACPI
    /// table-loader blobs.
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
        let smbios = build_smbios(cfg.cpu_count, cfg.ram_size);
        fw_cfg.add_file(SMBIOS_ANCHOR_FILE, smbios.anchor);
        fw_cfg.add_file(SMBIOS_TABLE_FILE, smbios.tables);
        Self {
            cfg,
            fw_cfg,
            uart: Pl011::new(),
            rtc: Pl031::new(),
            pcie: PcieEcam::new(),
            nvme: NvmeController::new(DEFAULT_NVME_DISK_BYTES),
            virtio_iso: None,
            pci_boot_media: None,
            flash_vars: P30NorFlash::new(
                machine::FLASH_VARS.base,
                machine::FLASH_VARS.size as usize,
                0x40000,
            ),
            pending_msix: Vec::new(),
            pending_spi_levels: Vec::new(),
            dtb,
        }
    }

    /// Load the writable pflash bank backing bytes. Live HVF code leaves the vars
    /// bank unmapped so NOR command/status reads and writes trap here instead of
    /// being treated as plain RAM stores.
    pub fn load_flash_vars(&mut self, data: &[u8]) {
        self.flash_vars.load(data);
    }

    /// Snapshot the writable pflash variable bank, including guest writes
    /// accepted through the NOR command protocol.
    pub fn flash_vars_image(&self) -> &[u8] {
        self.flash_vars.image()
    }

    /// Replace the first NVMe disk image. The image is padded to full 512-byte
    /// LBAs by the controller.
    pub fn load_nvme_disk(&mut self, data: Vec<u8>) {
        self.nvme.load_disk_image(data);
    }

    /// Attach a host raw disk file as the first NVMe namespace without reading
    /// the whole image into memory. With `write_back = false`, guest writes stay
    /// in a sparse overlay and are only visible through explicit export.
    pub fn attach_nvme_raw_file(
        &mut self,
        path: impl AsRef<Path>,
        write_back: bool,
    ) -> io::Result<()> {
        self.nvme.load_raw_file(path, write_back)
    }

    /// Attach a read-only Windows/Linux installer ISO to the last QEMU virt
    /// virtio-mmio transport slot. QEMU's own `virtio-blk-device` oracle uses
    /// slot 31 (`0x0a003e00`) for an explicitly added MMIO block device.
    pub fn attach_virtio_iso(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        self.virtio_iso = Some(VirtioMmioBlock::open_read_only(path)?);
        Ok(())
    }

    pub fn attach_pci_boot_media(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        self.pci_boot_media = Some(VirtioPciBlock::open_read_only(path)?);
        Ok(())
    }

    /// Current read/queue counters for the installer ISO block device, if one is
    /// attached. Live probes print this when diagnosing Windows media boot.
    pub fn virtio_iso_stats(&self) -> Option<VirtioMmioBlockStats> {
        self.virtio_iso.as_ref().map(VirtioMmioBlock::stats)
    }

    pub fn pci_boot_media_stats(&self) -> Option<VirtioMmioBlockStats> {
        self.pci_boot_media.as_ref().map(VirtioPciBlock::stats)
    }

    /// Snapshot the first NVMe disk image, including guest writes processed so
    /// far. Live probes use this to persist an explicitly writable image.
    pub fn nvme_disk(&self) -> &[u8] {
        self.nvme.disk_image()
    }

    /// In-memory snapshot of the first NVMe disk, if the media is memory-backed.
    pub fn nvme_disk_if_memory(&self) -> Option<&[u8]> {
        self.nvme.disk_image_if_memory()
    }

    /// Export current NVMe media to a raw file, applying sparse overlay writes.
    pub fn export_nvme_disk(&mut self, path: impl AsRef<Path>) -> io::Result<u64> {
        self.nvme.export_disk_image(path)
    }

    /// Flush write-through NVMe media.
    pub fn flush_nvme_disk(&mut self) -> io::Result<()> {
        self.nvme.flush_disk()
    }

    /// Current byte length of the NVMe namespace backing media.
    pub fn nvme_disk_len(&self) -> u64 {
        self.nvme.disk_len()
    }

    /// Recent NVMe commands processed by the controller, oldest first. Live
    /// probes use this to diagnose Windows setup stalls without enabling a
    /// firehose of per-command logging.
    pub fn nvme_command_trace(&self) -> Vec<NvmeCommandTrace> {
        self.nvme.recent_command_trace()
    }

    /// Resolve a guest-physical PCIe MMIO address against the currently
    /// programmed endpoint BARs without dispatching the access.
    pub fn pcie_mmio_target(&self, gpa: u64) -> Option<PcieMmioTarget> {
        self.pcie.mmio_target(gpa)
    }

    /// Drain MSI-X messages raised by PCIe devices since the last call. The live
    /// HVF run loop turns these into `hv_gic_send_msi` calls after configuring
    /// Apple `hv_gic`'s MSI frame.
    pub fn take_pending_msix(&mut self) -> Vec<MsixMessage> {
        std::mem::take(&mut self.pending_msix)
    }

    /// Drain level changes for legacy SPI-backed devices such as virtio-mmio.
    /// The live HVF loop turns these into `hv_gic_set_spi(intid, level)`.
    pub fn take_pending_spi_levels(&mut self) -> Vec<(u32, bool)> {
        std::mem::take(&mut self.pending_spi_levels)
    }

    /// Register QEMU direct-Linux-boot payloads in the fixed fw_cfg slots that
    /// ArmVirtQemu's `QemuKernelLoaderFsDxe` reads before BDS falls through to
    /// normal boot options. `cmdline` must include the terminating NUL byte.
    pub fn set_linux_boot_blobs(
        &mut self,
        kernel: Vec<u8>,
        initrd: Option<Vec<u8>>,
        cmdline: Vec<u8>,
    ) {
        assert!(
            cmdline.last().copied() == Some(0),
            "Linux fw_cfg cmdline blob must be NUL-terminated"
        );
        let initrd = initrd.unwrap_or_default();
        let kernel_len =
            u32::try_from(kernel.len()).expect("Linux kernel fw_cfg blob exceeds 4 GiB");
        let initrd_len =
            u32::try_from(initrd.len()).expect("Linux initrd fw_cfg blob exceeds 4 GiB");
        let cmdline_len =
            u32::try_from(cmdline.len()).expect("Linux cmdline fw_cfg blob exceeds 4 GiB");
        self.fw_cfg
            .add_item(KEY_KERNEL_SIZE, kernel_len.to_le_bytes().to_vec());
        self.fw_cfg.add_item(KEY_KERNEL_DATA, kernel);
        self.fw_cfg
            .add_item(KEY_INITRD_SIZE, initrd_len.to_le_bytes().to_vec());
        self.fw_cfg.add_item(KEY_INITRD_DATA, initrd);
        self.fw_cfg
            .add_item(KEY_CMDLINE_SIZE, cmdline_len.to_le_bytes().to_vec());
        self.fw_cfg.add_item(KEY_CMDLINE_DATA, cmdline);
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
        self.fw_cfg.add_file(SMBIOS_ANCHOR_FILE, anchor);
        self.fw_cfg.add_file(SMBIOS_TABLE_FILE, tables);
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
            "pcie-mmio-32" => self.pcie_mmio_access(gpa, op, mem),
            "virtio-mmio" => self.virtio_mmio_access(gpa - machine::VIRTIO_MMIO.base, op, mem),
            "flash-vars" => self.flash_vars.access(gpa, op),
            // Modelled in the machine map but no device behaviour yet — surfaced
            // precisely so bring-up traces show the next thing to implement.
            other => MmioOutcome::KnownUnimplemented(other),
        }
    }

    /// Empty virtio-mmio transport slot. Advertise a valid legacy register block with
    /// DeviceID 0 so the firmware sees "valid transport, no device" and skips it
    /// silently — matching QEMU's empty slots. Returning 0 (no magic) instead made
    /// VirtioMmioInit fail with EFI_UNSUPPORTED and log 32 errors per boot.
    fn virtio_mmio_access(
        &mut self,
        slot_offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let slot = slot_offset / machine::VIRTIO_MMIO_SLOT_SIZE;
        let reg = slot_offset % machine::VIRTIO_MMIO_SLOT_SIZE;
        if slot == INSTALLER_ISO_SLOT {
            if let Some(dev) = self.virtio_iso.as_mut() {
                let (is_write, size, value) = match op {
                    MmioOp::Read { size } => (false, size, 0),
                    MmioOp::Write { size, value } => (true, size, value),
                };
                let old_irq = dev.interrupt_line_level();
                let result = dev.access(reg, is_write, size, value, mem);
                let new_irq = dev.interrupt_line_level();
                if old_irq != new_irq {
                    self.pending_spi_levels.push((
                        machine::spi_to_intid(machine::virtio_mmio_spi(INSTALLER_ISO_SLOT as u32)),
                        new_irq,
                    ));
                }
                return match result {
                    VirtioMmioBlockResult::ReadValue(v) => MmioOutcome::ReadValue(v),
                    VirtioMmioBlockResult::WriteAck => MmioOutcome::WriteAck,
                };
            }
        }
        match op {
            MmioOp::Read { .. } => {
                let value = match reg {
                    0x00 => 0x7472_6976, // MagicValue, "virt"
                    0x04 => 0x1,         // Version: virtio-mmio 0.9.5
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
                self.flush_nvme_pending_msix();
                MmioOutcome::WriteAck
            }
        }
    }

    /// PCIe BAR memory-space access through the 32-bit MMIO aperture. Today the
    /// only endpoint is the NVMe controller at `00:01.0` BAR0.
    fn pcie_mmio_access(
        &mut self,
        gpa: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(target) = self.pcie.mmio_target(gpa) else {
            return MmioOutcome::KnownUnimplemented("pcie-mmio-32");
        };
        match (target.bdf, target.bar_index) {
            (NVME_BDF, 0) => self.nvme_access(target.offset, op, mem),
            (VIRTIO_BLK_BDF, 0) => self.pci_boot_media_access(target.offset, op, mem),
            _ => MmioOutcome::KnownUnimplemented("pcie-mmio-32"),
        }
    }

    fn pci_boot_media_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(dev) = self.pci_boot_media.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-blk-pci");
        };
        let old_irq = dev.interrupt_line_level();
        let result = match op {
            MmioOp::Read { size } => dev.access(offset, VirtioPciBlockOp::Read { size }, mem),
            MmioOp::Write { size, value } => {
                dev.access(offset, VirtioPciBlockOp::Write { size, value }, mem)
            }
        };
        let new_irq = dev.interrupt_line_level();
        if old_irq != new_irq {
            self.pending_spi_levels
                .push((machine::spi_to_intid(machine::SPI_PCIE_INTA), new_irq));
        }
        match result {
            VirtioMmioBlockResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioMmioBlockResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    fn nvme_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.nvme.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.nvme.mmio_write(offset, size, value);
                let completions = self.nvme.process(mem);
                self.queue_nvme_completion_msix(completions);
                self.flush_nvme_pending_msix();
                MmioOutcome::WriteAck
            }
        }
    }

    fn queue_nvme_completion_msix(&mut self, completions: Vec<NvmeCompletionEvent>) {
        let control = self.pcie.nvme_msix_control();
        for completion in completions {
            if let Some(message) =
                self.nvme
                    .raise_msix(completion.vector, control.enabled, control.function_masked)
            {
                self.pending_msix.push(message);
            }
        }
    }

    fn flush_nvme_pending_msix(&mut self) {
        let control = self.pcie.nvme_msix_control();
        self.pending_msix.extend(
            self.nvme
                .drain_pending_msix(control.enabled, control.function_masked),
        );
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

    /// Queue bytes that the guest can read from the PL011 UART data register.
    /// Live probes use this to test firmware/loader input paths while the default
    /// platform remains an unattached, receive-empty serial backend.
    pub fn push_uart_input(&mut self, bytes: &[u8]) {
        self.uart.push_input(bytes);
    }

    /// Number of preloaded PL011 input bytes still waiting to be read.
    pub fn uart_input_len(&self) -> usize {
        self.uart.input_len()
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
    use std::{fs, path::PathBuf, time::SystemTime};

    const REG_DATA: u64 = 0x0;
    const REG_SELECTOR: u64 = 0x8;
    const REG_DMA: u64 = 0x10;

    fn platform() -> VirtPlatform {
        VirtPlatform::new(VirtFdtConfig::default())
    }

    fn pcie_cfg_gpa(device: u8, function: u8, reg: u16) -> u64 {
        machine::PCIE_ECAM.base
            + (u64::from(device) << 15)
            + (u64::from(function) << 12)
            + u64::from(reg)
    }

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "bridgevm-hvf-platform-virt-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn write_vring_desc(
        mem: &mut FlatGuestRam,
        table: u64,
        index: u16,
        addr: u64,
        len: u32,
        flags: u16,
        next: u16,
    ) {
        let gpa = table + u64::from(index) * 16;
        assert!(mem.write_bytes(gpa, &addr.to_le_bytes()));
        assert!(mem.write_bytes(gpa + 8, &len.to_le_bytes()));
        assert!(mem.write_bytes(gpa + 12, &flags.to_le_bytes()));
        assert!(mem.write_bytes(gpa + 14, &next.to_le_bytes()));
    }

    fn program_nvme_bar0(p: &mut VirtPlatform, mem: &mut FlatGuestRam) {
        p.on_mmio(
            pcie_cfg_gpa(1, 0, crate::pcie::REG_BAR0),
            MmioOp::Write {
                size: 4,
                value: machine::PCIE_MMIO_32.base,
            },
            mem,
        );
        p.on_mmio(
            pcie_cfg_gpa(1, 0, crate::pcie::REG_COMMAND_STATUS),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            },
            mem,
        );
    }

    fn program_virtio_blk_bar0(p: &mut VirtPlatform, mem: &mut FlatGuestRam, base: u64) {
        p.on_mmio(
            pcie_cfg_gpa(3, 0, crate::pcie::REG_BAR0),
            MmioOp::Write {
                size: 4,
                value: base,
            },
            mem,
        );
        p.on_mmio(
            pcie_cfg_gpa(3, 0, crate::pcie::REG_COMMAND_STATUS),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            },
            mem,
        );
    }

    fn encode_nvme_sqe(
        opcode: u8,
        command_id: u16,
        nsid: u32,
        prp1: u64,
        cdw10: u32,
        cdw11: u32,
        cdw12: u32,
    ) -> [u8; 64] {
        let mut e = [0u8; 64];
        let cdw0 = u32::from(opcode) | (u32::from(command_id) << 16);
        e[0..4].copy_from_slice(&cdw0.to_le_bytes());
        e[4..8].copy_from_slice(&nsid.to_le_bytes());
        e[24..32].copy_from_slice(&prp1.to_le_bytes());
        e[40..44].copy_from_slice(&cdw10.to_le_bytes());
        e[44..48].copy_from_slice(&cdw11.to_le_bytes());
        e[48..52].copy_from_slice(&cdw12.to_le_bytes());
        e
    }

    fn nvme_mmio_write(
        p: &mut VirtPlatform,
        mem: &mut FlatGuestRam,
        offset: u64,
        size: u8,
        value: u64,
    ) {
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + offset,
                MmioOp::Write { size, value },
                mem,
            ),
            MmioOutcome::WriteAck
        );
    }

    fn enable_nvme_controller(p: &mut VirtPlatform, mem: &mut FlatGuestRam, asq: u64, acq: u64) {
        let qdepth = 4u64;
        nvme_mmio_write(
            p,
            mem,
            crate::nvme::REG_AQA,
            4,
            ((qdepth - 1) << 16) | (qdepth - 1),
        );
        nvme_mmio_write(p, mem, crate::nvme::REG_ASQ, 8, asq);
        nvme_mmio_write(p, mem, crate::nvme::REG_ACQ, 8, acq);
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_CC,
                MmioOp::Write { size: 4, value: 1 },
                mem,
            ),
            MmioOutcome::WriteAck
        );
    }

    fn enable_nvme_msix_vector0(
        p: &mut VirtPlatform,
        mem: &mut FlatGuestRam,
        address: u64,
        data: u32,
    ) {
        let table = u64::from(crate::pcie::NVME_MSIX_TABLE_OFFSET);
        nvme_mmio_write(p, mem, table, 8, address);
        nvme_mmio_write(p, mem, table + 8, 4, u64::from(data));
        nvme_mmio_write(p, mem, table + 12, 4, 0);
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(1, 0, u16::from(crate::pcie::NVME_MSIX_CAP_OFFSET) + 2),
                MmioOp::Write {
                    size: 2,
                    value: 0x8000,
                },
                mem,
            ),
            MmioOutcome::WriteAck
        );
    }

    fn submit_admin_sqe(
        p: &mut VirtPlatform,
        mem: &mut FlatGuestRam,
        asq: u64,
        slot: u16,
        sqe: &[u8; 64],
    ) {
        assert!(mem.write_bytes(asq + u64::from(slot) * crate::nvme::SQ_ENTRY_SIZE, sqe));
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE,
                MmioOp::Write {
                    size: 4,
                    value: u64::from(slot + 1),
                },
                mem,
            ),
            MmioOutcome::WriteAck
        );
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
        // An empty slot (device 2, ECAM offset dev<<15) reads all-ones (no device).
        assert_eq!(
            p.on_mmio(
                machine::PCIE_ECAM.base + (2 << 15),
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(0xFFFF_FFFF)
        );
    }

    #[test]
    fn pcie_nvme_endpoint_routes_bar0_to_controller_registers() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);

        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(1, 0, crate::pcie::REG_VENDOR_DEVICE),
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(0x0010_1b36)
        );
        assert!(matches!(
            p.on_mmio(
                pcie_cfg_gpa(1, 0, crate::pcie::REG_REVISION_CLASS),
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(v) if v >> 8 == u64::from(crate::pcie::NVME_CLASS_CODE)
        ));

        p.on_mmio(
            pcie_cfg_gpa(1, 0, crate::pcie::REG_BAR0),
            MmioOp::Write {
                size: 4,
                value: 0xFFFF_FFFF,
            },
            &mut mem,
        );
        let MmioOutcome::ReadValue(mask) = p.on_mmio(
            pcie_cfg_gpa(1, 0, crate::pcie::REG_BAR0),
            MmioOp::Read { size: 4 },
            &mut mem,
        ) else {
            panic!("BAR0 sizing read did not return a value");
        };
        let size = (!((mask as u32) & !0xF)).wrapping_add(1);
        assert_eq!(size, crate::pcie::NVME_BAR0_SIZE);

        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_VS,
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::KnownUnimplemented("pcie-mmio-32")
        );
        program_nvme_bar0(&mut p, &mut mem);
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_VS,
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(u64::from(crate::nvme::NVME_VERSION_1_4_0))
        );
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_CC,
                MmioOp::Write { size: 4, value: 1 },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_CSTS,
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(1)
        );
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

        let identify_controller = encode_nvme_sqe(0x06, 7, 0, DATA, 0x01, 0, 0);
        submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &identify_controller);

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

    #[test]
    fn virtio_iso_completion_queues_legacy_spi_level_changes() {
        const REG_GUEST_PAGE_SIZE: u64 = 0x28;
        const REG_QUEUE_NUM: u64 = 0x38;
        const REG_QUEUE_ALIGN: u64 = 0x3c;
        const REG_QUEUE_PFN: u64 = 0x40;
        const REG_QUEUE_NOTIFY: u64 = 0x50;
        const REG_INTERRUPT_ACK: u64 = 0x64;
        const DESC_F_NEXT: u16 = 1;
        const DESC_F_WRITE: u16 = 2;
        const VIRTIO_BLK_T_IN: u32 = 0;
        const VIRTIO_BLK_S_OK: u8 = 0;

        let path = temp_path("virtio-iso");
        let mut media = vec![0u8; 1024];
        media[512..520].copy_from_slice(b"WINSETUP");
        fs::write(&path, media).unwrap();

        let mut p = platform();
        p.attach_virtio_iso(&path).unwrap();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x10000);
        let slot_base = machine::virtio_mmio_slot(INSTALLER_ISO_SLOT).base;
        let desc = machine::RAM_BASE + 0x1000;
        let avail = desc + 8 * 16;
        let used = (avail + 4 + 8 * 2).div_ceil(4096) * 4096;
        let header = machine::RAM_BASE + 0x4000;
        let data = machine::RAM_BASE + 0x5000;
        let status = machine::RAM_BASE + 0x6000;

        assert!(mem.write_bytes(header, &VIRTIO_BLK_T_IN.to_le_bytes()));
        assert!(mem.write_bytes(header + 8, &1u64.to_le_bytes()));
        write_vring_desc(&mut mem, desc, 0, header, 16, DESC_F_NEXT, 1);
        write_vring_desc(&mut mem, desc, 1, data, 512, DESC_F_NEXT | DESC_F_WRITE, 2);
        write_vring_desc(&mut mem, desc, 2, status, 1, DESC_F_WRITE, 0);
        assert!(mem.write_bytes(avail + 2, &1u16.to_le_bytes()));
        assert!(mem.write_bytes(avail + 4, &0u16.to_le_bytes()));

        for (reg, value) in [
            (REG_QUEUE_NUM, 8),
            (REG_GUEST_PAGE_SIZE, 4096),
            (REG_QUEUE_ALIGN, 4096),
            (REG_QUEUE_PFN, desc >> 12),
        ] {
            assert_eq!(
                p.on_mmio(slot_base + reg, MmioOp::Write { size: 4, value }, &mut mem),
                MmioOutcome::WriteAck
            );
        }
        assert_eq!(
            p.on_mmio(
                slot_base + REG_QUEUE_NOTIFY,
                MmioOp::Write { size: 4, value: 0 },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );

        assert_eq!(mem.read_bytes(data, 8).unwrap(), b"WINSETUP");
        assert_eq!(mem.read_bytes(status, 1).unwrap(), [VIRTIO_BLK_S_OK]);
        assert_eq!(
            u16::from_le_bytes(mem.read_bytes(used + 2, 2).unwrap().try_into().unwrap()),
            1
        );
        assert_eq!(
            p.take_pending_spi_levels(),
            vec![(
                machine::spi_to_intid(machine::virtio_mmio_spi(INSTALLER_ISO_SLOT as u32)),
                true
            )]
        );

        assert_eq!(
            p.on_mmio(
                slot_base + REG_INTERRUPT_ACK,
                MmioOp::Write { size: 4, value: 1 },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.take_pending_spi_levels(),
            vec![(
                machine::spi_to_intid(machine::virtio_mmio_spi(INSTALLER_ISO_SLOT as u32)),
                false
            )]
        );

        fs::remove_file(path).ok();
    }

    #[test]
    fn pcie_boot_media_reads_from_attached_iso_and_posts_interrupt() {
        const PCI_ISR_CFG_OFFSET: u64 = 0x1000;
        const PCI_NOTIFY_CFG_OFFSET: u64 = 0x3000;
        const REG_QUEUE_NUM: u64 = 0x038;
        const REG_QUEUE_READY: u64 = 0x044;
        const REG_QUEUE_NOTIFY: u64 = 0x050;
        const REG_QUEUE_DESC_LOW: u64 = 0x080;
        const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
        const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;
        const DESC_F_NEXT: u16 = 1;
        const DESC_F_WRITE: u16 = 2;
        const VIRTIO_BLK_T_IN: u32 = 0;
        const VIRTIO_BLK_S_OK: u8 = 0;

        let path = temp_path("pci-boot-media");
        let mut media = vec![0u8; 1024];
        media[512..520].copy_from_slice(b"WINSETUP");
        fs::write(&path, media).unwrap();

        let mut p = platform();
        p.attach_pci_boot_media(&path).unwrap();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x10000);
        let bar = machine::PCIE_MMIO_32.base + 0x8000;
        program_virtio_blk_bar0(&mut p, &mut mem, bar);

        let desc = machine::RAM_BASE + 0x1000;
        let avail = machine::RAM_BASE + 0x2000;
        let used = machine::RAM_BASE + 0x3000;
        let header = machine::RAM_BASE + 0x4000;
        let data = machine::RAM_BASE + 0x5000;
        let status = machine::RAM_BASE + 0x6000;

        assert!(mem.write_bytes(header, &VIRTIO_BLK_T_IN.to_le_bytes()));
        assert!(mem.write_bytes(header + 8, &1u64.to_le_bytes()));
        write_vring_desc(&mut mem, desc, 0, header, 16, DESC_F_NEXT, 1);
        write_vring_desc(&mut mem, desc, 1, data, 512, DESC_F_NEXT | DESC_F_WRITE, 2);
        write_vring_desc(&mut mem, desc, 2, status, 1, DESC_F_WRITE, 0);
        assert!(mem.write_bytes(avail + 2, &1u16.to_le_bytes()));
        assert!(mem.write_bytes(avail + 4, &0u16.to_le_bytes()));

        for (reg, value) in [
            (REG_QUEUE_NUM, 8),
            (REG_QUEUE_DESC_LOW, desc),
            (REG_QUEUE_DRIVER_LOW, avail),
            (REG_QUEUE_DEVICE_LOW, used),
            (REG_QUEUE_READY, 1),
        ] {
            assert_eq!(
                p.on_mmio(bar + reg, MmioOp::Write { size: 4, value }, &mut mem),
                MmioOutcome::WriteAck
            );
        }
        assert_eq!(
            p.on_mmio(
                bar + PCI_NOTIFY_CFG_OFFSET + REG_QUEUE_NOTIFY,
                MmioOp::Write { size: 4, value: 0 },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );

        assert_eq!(mem.read_bytes(data, 8).unwrap(), b"WINSETUP");
        assert_eq!(mem.read_bytes(status, 1).unwrap(), [VIRTIO_BLK_S_OK]);
        assert_eq!(
            p.take_pending_spi_levels(),
            vec![(machine::spi_to_intid(machine::SPI_PCIE_INTA), true)]
        );

        assert_eq!(
            p.on_mmio(
                bar + PCI_ISR_CFG_OFFSET,
                MmioOp::Write { size: 4, value: 1 },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.take_pending_spi_levels(),
            vec![(machine::spi_to_intid(machine::SPI_PCIE_INTA), false)]
        );

        fs::remove_file(path).ok();
    }

    #[test]
    fn unknown_pcie_bar_still_reports_known_unimplemented() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);

        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + 0x1_0000,
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::KnownUnimplemented("pcie-mmio-32")
        );
    }

    #[test]
    fn pcie_nvme_reads_and_writes_preloaded_disk_media() {
        const ASQ: u64 = machine::RAM_BASE + 0x1000;
        const ACQ: u64 = machine::RAM_BASE + 0x2000;
        const IO_SQ: u64 = machine::RAM_BASE + 0x3000;
        const IO_CQ: u64 = machine::RAM_BASE + 0x4000;
        const DATA: u64 = machine::RAM_BASE + 0x5000;
        const SLBA: u64 = 7;

        let mut p = platform();
        let mut disk = vec![0u8; crate::nvme::LBA_SIZE * 16];
        let pattern: Vec<u8> = (0..crate::nvme::LBA_SIZE)
            .map(|i| 0x80 | ((i % 0x40) as u8))
            .collect();
        let start = SLBA as usize * crate::nvme::LBA_SIZE;
        disk[start..start + crate::nvme::LBA_SIZE].copy_from_slice(&pattern);
        p.load_nvme_disk(disk);

        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x9000);
        program_nvme_bar0(&mut p, &mut mem);
        enable_nvme_controller(&mut p, &mut mem, ASQ, ACQ);

        let cdw10 = (3u32 << 16) | 1;
        let create_cq = encode_nvme_sqe(0x05, 1, 0, IO_CQ, cdw10, 1, 0);
        submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &create_cq);
        let create_sq = encode_nvme_sqe(0x01, 2, 0, IO_SQ, cdw10, 1u32 << 16, 0);
        submit_admin_sqe(&mut p, &mut mem, ASQ, 1, &create_sq);

        let read = encode_nvme_sqe(
            0x02,
            0x10,
            crate::nvme::NSID,
            DATA,
            SLBA as u32,
            (SLBA >> 32) as u32,
            0,
        );
        assert!(mem.write_bytes(IO_SQ, &read));
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE + 2 * 4,
                MmioOp::Write { size: 4, value: 1 },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            mem.read_bytes(DATA, crate::nvme::LBA_SIZE).unwrap(),
            pattern
        );

        let replacement: Vec<u8> = (0..crate::nvme::LBA_SIZE)
            .map(|i| 0x40 | ((i % 0x20) as u8))
            .collect();
        assert!(mem.write_bytes(DATA, &replacement));
        let write = encode_nvme_sqe(
            0x01,
            0x11,
            crate::nvme::NSID,
            DATA,
            SLBA as u32,
            (SLBA >> 32) as u32,
            0,
        );
        assert!(mem.write_bytes(IO_SQ + crate::nvme::SQ_ENTRY_SIZE, &write));
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE + 2 * 4,
                MmioOp::Write { size: 4, value: 2 },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            &p.nvme_disk()[start..start + crate::nvme::LBA_SIZE],
            replacement.as_slice()
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
        // UARTFR (offset 0x18) reports idle FIFOs: TXFE and RXFE set.
        assert!(matches!(
            p.on_mmio(machine::UART.base + 0x18, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(v) if v & ((1 << 7) | (1 << 4)) == ((1 << 7) | (1 << 4))
        ));
    }

    #[test]
    fn uart_reads_consume_preloaded_input_via_mmio() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        p.push_uart_input(b" ");
        assert_eq!(p.uart_input_len(), 1);
        assert!(matches!(
            p.on_mmio(machine::UART.base + 0x18, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(v) if v & (1 << 4) == 0
        ));
        assert_eq!(
            p.on_mmio(machine::UART.base, MmioOp::Read { size: 1 }, &mut mem),
            MmioOutcome::ReadValue(u64::from(b' '))
        );
        assert_eq!(p.uart_input_len(), 0);
        assert!(matches!(
            p.on_mmio(machine::UART.base + 0x18, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(v) if v & (1 << 4) != 0
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
    fn flash_vars_snapshot_reflects_guest_programming() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        p.load_flash_vars(&[0xff; 8]);
        assert_eq!(p.flash_vars_image()[0], 0xff);

        assert_eq!(
            p.on_mmio(
                machine::FLASH_VARS.base,
                MmioOp::Write {
                    size: 4,
                    value: 0x0040_0040,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                machine::FLASH_VARS.base,
                MmioOp::Write {
                    size: 4,
                    value: 0x1234_5678,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(&p.flash_vars_image()[0..4], &[0x78, 0x56, 0x34, 0x12]);
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
    fn generated_smbios_tables_are_registered_by_default() {
        let mut p = platform();
        p.fw_cfg.select(crate::fwcfg::KEY_FILE_DIR);
        let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
        let blob = String::from_utf8_lossy(&dir);
        for name in [SMBIOS_ANCHOR_FILE, SMBIOS_TABLE_FILE] {
            assert!(blob.contains(name), "default fw_cfg dir missing {name}");
        }
    }

    #[test]
    fn linux_boot_blobs_register_qemu_numeric_fw_cfg_items() {
        let mut p = platform();
        p.set_linux_boot_blobs(
            b"kernel-image".to_vec(),
            Some(b"initrd-image".to_vec()),
            b"console=ttyAMA0\0".to_vec(),
        );

        p.fw_cfg.select(KEY_KERNEL_SIZE);
        assert_eq!(
            p.fw_cfg.mmio_read(0, 4),
            12,
            "QemuKernelLoaderFsDxe reads KERNEL_SIZE with QemuFwCfgRead32"
        );
        p.fw_cfg.select(KEY_KERNEL_DATA);
        assert_eq!(p.fw_cfg.read_data(12), b"kernel-image");

        p.fw_cfg.select(KEY_INITRD_SIZE);
        assert_eq!(p.fw_cfg.mmio_read(0, 4), 12);
        p.fw_cfg.select(KEY_INITRD_DATA);
        assert_eq!(p.fw_cfg.read_data(12), b"initrd-image");

        p.fw_cfg.select(KEY_CMDLINE_SIZE);
        assert_eq!(p.fw_cfg.mmio_read(0, 4), 16);
        p.fw_cfg.select(KEY_CMDLINE_DATA);
        assert_eq!(p.fw_cfg.read_data(16), b"console=ttyAMA0\0");

        p.fw_cfg.select(crate::fwcfg::KEY_FILE_DIR);
        let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
        let blob = String::from_utf8_lossy(&dir);
        assert!(!blob.contains("kernel-image"));
        assert!(!blob.contains("initrd-image"));
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
            "etc/smbios/smbios-tables",
            "bootorder",
        ] {
            assert!(blob.contains(name), "fw_cfg dir missing {name}");
        }
        // Suppress unused-variable warning for `mem` in this assertion-only test.
        let _ = &mut mem;
    }
}
