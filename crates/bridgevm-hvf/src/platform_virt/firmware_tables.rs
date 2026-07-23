//! Firmware handoff: fw_cfg blob registration, DTB, fw_cfg MMIO, ramfb config, pflash var store.

use super::*;
use crate::acpi::ACPI_LOADER_FILE;
use crate::acpi::ACPI_RSDP_FILE;
use crate::acpi::ACPI_TABLE_FILE;
use crate::fwcfg::GuestMemoryMut;
use crate::fwcfg::KEY_CMDLINE_DATA;
use crate::fwcfg::KEY_CMDLINE_SIZE;
use crate::fwcfg::KEY_INITRD_DATA;
use crate::fwcfg::KEY_INITRD_SIZE;
use crate::fwcfg::KEY_KERNEL_DATA;
use crate::fwcfg::KEY_KERNEL_SIZE;
use crate::ramfb::RamfbConfig;
use crate::ramfb::RAMFB_FW_CFG_FILE;
use crate::smbios::SMBIOS_ANCHOR_FILE;
use crate::smbios::SMBIOS_TABLE_FILE;

impl VirtPlatform {
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

    pub fn ramfb_config(&self) -> Option<RamfbConfig> {
        if !self.devices.ramfb_present {
            return None;
        }
        self.ramfb.config()
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
        // SAFE-EXPECT: fw_cfg direct-boot size registers are u32 by QEMU contract.
        let kernel_len = u32::try_from(kernel.len()).expect("kernel blob >4 GiB");
        // SAFE-EXPECT: fw_cfg direct-boot size registers are u32 by QEMU contract.
        let initrd_len = u32::try_from(initrd.len()).expect("initrd blob >4 GiB");
        // SAFE-EXPECT: fw_cfg direct-boot size registers are u32 by QEMU contract.
        let cmdline_len = u32::try_from(cmdline.len()).expect("cmdline blob >4 GiB");
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

    pub(crate) fn fw_cfg_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.fw_cfg.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.fw_cfg.mmio_write(offset, size, value, mem);
                self.refresh_ramfb();
                MmioOutcome::WriteAck
            }
        }
    }

    pub(crate) fn refresh_ramfb(&mut self) {
        if !self.devices.ramfb_present {
            return;
        }
        if let Some(bytes) = self.fw_cfg.file_bytes(RAMFB_FW_CFG_FILE) {
            self.ramfb.update_from_fw_cfg(bytes);
        }
    }
}
