//! Flattened Device Tree (FDT/DTB) builder and a QEMU-`virt`-shaped generator.
//!
//! Path A hands the stock ArmVirtQemu firmware a device tree describing the
//! platform; the firmware parses it (memory, CPUs, GICv3, `fw_cfg`, PCIe, virtio)
//! and emits ACPI for the guest. The legacy probe harness builds a minimal,
//! non-QEMU DTB (no `fw_cfg`/PCIe nodes); this module replaces it with a faithful
//! QEMU-`virt` tree generated from the single source of truth in
//! [`crate::machine`].
//!
//! [`FdtBuilder`] is a generic DTB v17 serializer (big-endian, 4-byte aligned
//! tokens, deduplicated strings block). [`build_virt_fdt`] uses it to emit the
//! essential `virt` nodes. The output is structurally valid and `dtc`-clean;
//! firmware acceptance on an entitled host is the next milestone (the live HVF
//! bring-up in `docs/hvf-windows-engine-strategy.md`).
//!
//! Reference: the DTB binary format (`dtspec`) and
//! `docs/reference/qemu-virt-aarch64-gicv3.dts`.

use std::collections::BTreeMap;

use crate::machine;

const FDT_MAGIC: u32 = 0xd00d_feed;
const FDT_VERSION: u32 = 17;
const FDT_LAST_COMP_VERSION: u32 = 16;

const FDT_BEGIN_NODE: u32 = 0x1;
const FDT_END_NODE: u32 = 0x2;
const FDT_PROP: u32 = 0x3;
const FDT_END: u32 = 0x9;

/// A generic flattened device tree (v17) builder.
#[derive(Debug, Default)]
pub struct FdtBuilder {
    structure: Vec<u8>,
    strings: Vec<u8>,
    string_offsets: BTreeMap<String, u32>,
    mem_rsv: Vec<(u64, u64)>,
    depth: i32,
}

fn pad_to_4(v: &mut Vec<u8>) {
    while v.len() % 4 != 0 {
        v.push(0);
    }
}

impl FdtBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a memory-reservation entry (rarely needed for `virt`, but part of the
    /// format).
    pub fn reserve_memory(&mut self, address: u64, size: u64) {
        self.mem_rsv.push((address, size));
    }

    pub fn begin_node(&mut self, name: &str) {
        self.structure
            .extend_from_slice(&FDT_BEGIN_NODE.to_be_bytes());
        self.structure.extend_from_slice(name.as_bytes());
        self.structure.push(0);
        pad_to_4(&mut self.structure);
        self.depth += 1;
    }

    pub fn end_node(&mut self) {
        self.structure
            .extend_from_slice(&FDT_END_NODE.to_be_bytes());
        self.depth -= 1;
    }

    fn intern(&mut self, name: &str) -> u32 {
        if let Some(off) = self.string_offsets.get(name) {
            return *off;
        }
        let off = self.strings.len() as u32;
        self.strings.extend_from_slice(name.as_bytes());
        self.strings.push(0);
        self.string_offsets.insert(name.to_string(), off);
        off
    }

    /// Raw property with an arbitrary byte value.
    pub fn prop_bytes(&mut self, name: &str, value: &[u8]) {
        let nameoff = self.intern(name);
        self.structure.extend_from_slice(&FDT_PROP.to_be_bytes());
        self.structure
            .extend_from_slice(&(value.len() as u32).to_be_bytes());
        self.structure.extend_from_slice(&nameoff.to_be_bytes());
        self.structure.extend_from_slice(value);
        pad_to_4(&mut self.structure);
    }

    /// Valueless property (e.g. `interrupt-controller;`).
    pub fn prop_empty(&mut self, name: &str) {
        self.prop_bytes(name, &[]);
    }

    pub fn prop_u32(&mut self, name: &str, value: u32) {
        self.prop_bytes(name, &value.to_be_bytes());
    }

    /// A property holding a list of big-endian `u32` cells.
    pub fn prop_cells(&mut self, name: &str, cells: &[u32]) {
        let mut buf = Vec::with_capacity(cells.len() * 4);
        for c in cells {
            buf.extend_from_slice(&c.to_be_bytes());
        }
        self.prop_bytes(name, &buf);
    }

    /// A single `<addr size>` reg pair encoded as 2 + 2 cells (the `virt`
    /// root uses `#address-cells = <2>`, `#size-cells = <2>`).
    pub fn prop_reg64(&mut self, name: &str, base: u64, size: u64) {
        self.prop_cells(
            name,
            &[
                (base >> 32) as u32,
                base as u32,
                (size >> 32) as u32,
                size as u32,
            ],
        );
    }

    /// Null-terminated string property.
    pub fn prop_str(&mut self, name: &str, value: &str) {
        let mut buf = value.as_bytes().to_vec();
        buf.push(0);
        self.prop_bytes(name, &buf);
    }

    /// Property holding a `\0`-separated string list (e.g. `compatible`).
    pub fn prop_str_list(&mut self, name: &str, values: &[&str]) {
        let mut buf = Vec::new();
        for v in values {
            buf.extend_from_slice(v.as_bytes());
            buf.push(0);
        }
        self.prop_bytes(name, &buf);
    }

    /// Serialize to a DTB blob.
    pub fn finish(mut self) -> Vec<u8> {
        assert_eq!(self.depth, 0, "unbalanced begin_node/end_node");
        self.structure.extend_from_slice(&FDT_END.to_be_bytes());

        const HEADER_LEN: u32 = 40;
        let off_mem_rsvmap = HEADER_LEN; // already 8-aligned
        let mut mem_rsv = Vec::new();
        for (a, s) in &self.mem_rsv {
            mem_rsv.extend_from_slice(&a.to_be_bytes());
            mem_rsv.extend_from_slice(&s.to_be_bytes());
        }
        mem_rsv.extend_from_slice(&0u64.to_be_bytes()); // terminator
        mem_rsv.extend_from_slice(&0u64.to_be_bytes());

        let off_dt_struct = off_mem_rsvmap + mem_rsv.len() as u32;
        let size_dt_struct = self.structure.len() as u32;
        let off_dt_strings = off_dt_struct + size_dt_struct;
        let size_dt_strings = self.strings.len() as u32;
        let totalsize = off_dt_strings + size_dt_strings;

        let mut out = Vec::with_capacity(totalsize as usize);
        for field in [
            FDT_MAGIC,
            totalsize,
            off_dt_struct,
            off_dt_strings,
            off_mem_rsvmap,
            FDT_VERSION,
            FDT_LAST_COMP_VERSION,
            0, // boot_cpuid_phys
            size_dt_strings,
            size_dt_struct,
        ] {
            out.extend_from_slice(&field.to_be_bytes());
        }
        out.extend_from_slice(&mem_rsv);
        out.extend_from_slice(&self.structure);
        out.extend_from_slice(&self.strings);
        out
    }
}

/// Configuration for [`build_virt_fdt`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtFdtConfig {
    pub cpu_count: u64,
    pub ram_size: u64,
}

impl Default for VirtFdtConfig {
    fn default() -> Self {
        Self {
            cpu_count: 4,
            ram_size: 6 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtFdtDeviceConfig {
    pub legacy_virtio_mmio_present: bool,
}

impl Default for VirtFdtDeviceConfig {
    fn default() -> Self {
        Self {
            legacy_virtio_mmio_present: true,
        }
    }
}

// Phandles (internally consistent; values are arbitrary but referenced below).
const PHANDLE_GIC: u32 = 0x1;
const PHANDLE_GIC_MSI_FRAME: u32 = 0x2;
const PHANDLE_APB_PCLK: u32 = 0x3;

// Device-tree interrupt encoding for the GICv3 (`#interrupt-cells = <3>`).
const IRQ_SPI: u32 = 0;
const IRQ_PPI: u32 = 1;
const IRQ_LEVEL_HI: u32 = 4;

/// Build a QEMU-`virt`-shaped device tree from [`crate::machine`].
pub fn build_virt_fdt(cfg: &VirtFdtConfig) -> Vec<u8> {
    build_virt_fdt_with_devices(cfg, VirtFdtDeviceConfig::default())
}

pub fn build_virt_fdt_with_devices(cfg: &VirtFdtConfig, devices: VirtFdtDeviceConfig) -> Vec<u8> {
    assert!(
        machine::redist_fits(cfg.cpu_count),
        "cpu_count {} exceeds GICv3 redistributor window",
        cfg.cpu_count
    );
    let mut b = FdtBuilder::new();

    b.begin_node(""); // root
    b.prop_u32("#address-cells", 2);
    b.prop_u32("#size-cells", 2);
    b.prop_str_list("compatible", &["linux,dummy-virt"]);
    b.prop_str("model", "bridgevm-virt");
    b.prop_empty("dma-coherent");
    // All devices inherit the GIC as their interrupt parent (QEMU sets this on
    // the root); without it `dtc` warns and OS interrupt routing is ambiguous.
    b.prop_cells("interrupt-parent", &[PHANDLE_GIC]);

    // /chosen
    b.begin_node("chosen");
    b.prop_str("stdout-path", "/pl011@9000000");
    b.end_node();

    // /psci — power state coordination (CPU on/off, reset, poweroff).
    b.begin_node("psci");
    b.prop_str_list("compatible", &["arm,psci-1.0", "arm,psci-0.2", "arm,psci"]);
    b.prop_str("method", "hvc");
    b.end_node();

    // /apb-pclk — fixed clock feeding PL011/PL031.
    b.begin_node("apb-pclk");
    b.prop_str_list("compatible", &["fixed-clock"]);
    b.prop_u32("#clock-cells", 0);
    b.prop_u32("clock-frequency", 24_000_000);
    b.prop_str("clock-output-names", "clk24mhz");
    b.prop_u32("phandle", PHANDLE_APB_PCLK);
    b.end_node();

    // /memory
    b.begin_node("memory@40000000");
    b.prop_str("device_type", "memory");
    b.prop_reg64("reg", machine::RAM_BASE, cfg.ram_size);
    b.end_node();

    // /cpus
    b.begin_node("cpus");
    b.prop_u32("#address-cells", 1);
    b.prop_u32("#size-cells", 0);
    for i in 0..cfg.cpu_count {
        b.begin_node(&format!("cpu@{i:x}"));
        b.prop_str("device_type", "cpu");
        b.prop_str_list("compatible", &["arm,arm-v8"]);
        b.prop_str("enable-method", "psci");
        b.prop_cells("reg", &[i as u32]);
        b.end_node();
    }
    b.end_node();

    // /timer — architected generic timer PPIs.
    b.begin_node("timer");
    b.prop_str_list("compatible", &["arm,armv8-timer", "arm,armv7-timer"]);
    b.prop_empty("always-on");
    b.prop_cells(
        "interrupts",
        &[
            IRQ_PPI,
            machine::PPI_TIMER_SECURE,
            IRQ_LEVEL_HI,
            IRQ_PPI,
            machine::PPI_TIMER_NONSEC,
            IRQ_LEVEL_HI,
            IRQ_PPI,
            machine::PPI_TIMER_VIRT,
            IRQ_LEVEL_HI,
            IRQ_PPI,
            machine::PPI_TIMER_HYP,
            IRQ_LEVEL_HI,
        ],
    );
    b.end_node();

    // /intc — GICv3 distributor + redistributor, with the ITS child.
    b.begin_node("intc@8000000");
    b.prop_str_list("compatible", &["arm,gic-v3"]);
    b.prop_empty("interrupt-controller");
    b.prop_u32("#interrupt-cells", 3);
    b.prop_u32("#address-cells", 2);
    b.prop_u32("#size-cells", 2);
    b.prop_empty("ranges");
    b.prop_u32("#redistributor-regions", 1);
    b.prop_cells(
        "reg",
        &[
            (machine::GIC_DIST.base >> 32) as u32,
            machine::GIC_DIST.base as u32,
            (machine::GIC_DIST.size >> 32) as u32,
            machine::GIC_DIST.size as u32,
            (machine::GIC_REDIST.base >> 32) as u32,
            machine::GIC_REDIST.base as u32,
            (machine::GIC_REDIST.size >> 32) as u32,
            machine::GIC_REDIST.size as u32,
        ],
    );
    b.prop_u32("phandle", PHANDLE_GIC);

    b.begin_node("v2m@8080000");
    b.prop_str_list("compatible", &["arm,gic-v2m-frame"]);
    b.prop_empty("msi-controller");
    b.prop_reg64(
        "reg",
        machine::GIC_MSI_FRAME.base,
        machine::GIC_MSI_FRAME.size,
    );
    b.prop_u32("arm,msi-base-spi", machine::GIC_MSI_INTID_BASE);
    b.prop_u32("arm,msi-num-spis", machine::GIC_MSI_INTID_COUNT);
    b.prop_u32("phandle", PHANDLE_GIC_MSI_FRAME);
    b.end_node();

    b.end_node(); // intc

    // /pl011 UART
    b.begin_node("pl011@9000000");
    b.prop_str_list("compatible", &["arm,pl011", "arm,primecell"]);
    b.prop_reg64("reg", machine::UART.base, machine::UART.size);
    b.prop_cells("interrupts", &[IRQ_SPI, machine::SPI_UART, IRQ_LEVEL_HI]);
    b.prop_cells("clocks", &[PHANDLE_APB_PCLK, PHANDLE_APB_PCLK]);
    b.prop_str_list("clock-names", &["uartclk", "apb_pclk"]);
    b.end_node();

    // /pl031 RTC
    b.begin_node("pl031@9010000");
    b.prop_str_list("compatible", &["arm,pl031", "arm,primecell"]);
    b.prop_reg64("reg", machine::RTC.base, machine::RTC.size);
    b.prop_cells("interrupts", &[IRQ_SPI, machine::SPI_RTC, IRQ_LEVEL_HI]);
    b.prop_cells("clocks", &[PHANDLE_APB_PCLK]);
    b.prop_str_list("clock-names", &["apb_pclk"]);
    b.end_node();

    // /flash@0 — the two pflash banks (code + vars). ArmVirtPkg's VirtNorFlashDxe
    // parses this to locate the UEFI variable store; without it the flash driver
    // faults in DXE.
    b.begin_node("flash@0");
    b.prop_str_list("compatible", &["cfi-flash"]);
    b.prop_cells(
        "reg",
        &[
            (machine::FLASH_CODE.base >> 32) as u32,
            machine::FLASH_CODE.base as u32,
            (machine::FLASH_CODE.size >> 32) as u32,
            machine::FLASH_CODE.size as u32,
            (machine::FLASH_VARS.base >> 32) as u32,
            machine::FLASH_VARS.base as u32,
            (machine::FLASH_VARS.size >> 32) as u32,
            machine::FLASH_VARS.size as u32,
        ],
    );
    b.prop_u32("bank-width", 4);
    b.end_node();

    // /fw-cfg — the keystone (see crate::fwcfg).
    b.begin_node("fw-cfg@9020000");
    b.prop_str_list("compatible", &["qemu,fw-cfg-mmio"]);
    b.prop_reg64("reg", machine::FW_CFG.base, machine::FW_CFG.size);
    b.end_node();

    // /virtio_mmio × 32
    if devices.legacy_virtio_mmio_present {
        for i in 0..machine::VIRTIO_MMIO_COUNT {
            let slot = machine::virtio_mmio_slot(i);
            b.begin_node(&format!("virtio_mmio@{:x}", slot.base));
            b.prop_str_list("compatible", &["virtio,mmio"]);
            b.prop_reg64("reg", slot.base, slot.size);
            b.prop_cells(
                "interrupts",
                &[IRQ_SPI, machine::virtio_mmio_spi(i as u32), IRQ_LEVEL_HI],
            );
            b.end_node();
        }
    }

    // /pcie — ECAM root complex with INTx interrupt-map and MSI via the frame.
    build_pcie_node(&mut b);

    b.end_node(); // root
    b.finish()
}

fn build_pcie_node(b: &mut FdtBuilder) {
    b.begin_node("pcie@10000000");
    b.prop_str_list("compatible", &["pci-host-ecam-generic"]);
    b.prop_str("device_type", "pci");
    b.prop_u32("#address-cells", 3);
    b.prop_u32("#size-cells", 2);
    b.prop_u32("#interrupt-cells", 1);
    b.prop_u32("linux,pci-domain", 0);
    b.prop_cells("bus-range", &[0x0, 0xff]);
    b.prop_empty("dma-coherent");
    b.prop_cells("msi-parent", &[PHANDLE_GIC_MSI_FRAME]);
    b.prop_reg64("reg", machine::PCIE_ECAM.base, machine::PCIE_ECAM.size);

    // ranges: <pci-addr(3) cpu-addr(2) size(2)> for I/O, 32-bit MMIO and the
    // non-prefetchable and prefetchable 64-bit MMIO apertures. The HVF probe
    // creates every VM with the host's
    // maximum IPA size (40 bits on supported Apple Silicon), so the 512 GiB..
    // 1 TiB aperture is addressable. Firmware needs this window to place large
    // 64-bit BARs such as virtio-gpu's 1 GiB host-visible memory BAR. Keep the
    // two high apertures disjoint so firmware can allocate both ordinary 64-bit
    // BARs and prefetchable shared-memory BARs.
    let io = machine::PCIE_PIO;
    let m32 = machine::PCIE_MMIO_32;
    let m64_non_prefetch = machine::PCIE_MMIO_64_NON_PREFETCH;
    let m64_prefetch = machine::PCIE_MMIO_64_PREFETCH;
    b.prop_cells(
        "ranges",
        &[
            // I/O space (0x01000000)
            0x0100_0000,
            0x0,
            0x0,
            (io.base >> 32) as u32,
            io.base as u32,
            (io.size >> 32) as u32,
            io.size as u32,
            // 32-bit MMIO (0x02000000)
            0x0200_0000,
            0x0,
            m32.base as u32,
            (m32.base >> 32) as u32,
            m32.base as u32,
            (m32.size >> 32) as u32,
            m32.size as u32,
            // 64-bit non-prefetchable MMIO (0x03000000)
            0x0300_0000,
            (m64_non_prefetch.base >> 32) as u32,
            m64_non_prefetch.base as u32,
            (m64_non_prefetch.base >> 32) as u32,
            m64_non_prefetch.base as u32,
            (m64_non_prefetch.size >> 32) as u32,
            m64_non_prefetch.size as u32,
            // 64-bit prefetchable MMIO (0x43000000)
            0x4300_0000,
            (m64_prefetch.base >> 32) as u32,
            m64_prefetch.base as u32,
            (m64_prefetch.base >> 32) as u32,
            m64_prefetch.base as u32,
            (m64_prefetch.size >> 32) as u32,
            m64_prefetch.size as u32,
        ],
    );

    // INTx swizzle: for each of 4 slots, INTA..D map to SPI 3..6 rotated.
    b.prop_cells("interrupt-map-mask", &[0x1800, 0x0, 0x0, 0x7]);
    let mut map = Vec::new();
    for dev in 0u32..4 {
        for pin in 1u32..=4 {
            let spi = machine::SPI_PCIE_INTA + ((dev + pin - 1) % 4);
            map.extend_from_slice(&[
                dev << 11,
                0x0,
                0x0, // pci addr (device in bits 11..15)
                pin, // pci interrupt pin (1=INTA)
                PHANDLE_GIC,
                0x0,
                0x0,
                IRQ_SPI,
                spi,
                IRQ_LEVEL_HI,
            ]);
        }
    }
    b.prop_cells("interrupt-map", &map);
    b.end_node();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read a big-endian u32 at `off`.
    fn be32(b: &[u8], off: usize) -> u32 {
        u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
    }

    #[test]
    fn header_is_well_formed() {
        let mut b = FdtBuilder::new();
        b.begin_node("");
        b.prop_u32("#address-cells", 2);
        b.end_node();
        let dtb = b.finish();

        assert_eq!(be32(&dtb, 0), FDT_MAGIC);
        assert_eq!(
            be32(&dtb, 4) as usize,
            dtb.len(),
            "totalsize == byte length"
        );
        assert_eq!(be32(&dtb, 20), FDT_VERSION);
        assert_eq!(be32(&dtb, 24), FDT_LAST_COMP_VERSION);
    }

    #[test]
    fn strings_block_is_deduplicated() {
        let mut b = FdtBuilder::new();
        b.begin_node("");
        b.begin_node("a");
        b.prop_u32("reg", 1);
        b.end_node();
        b.begin_node("b");
        b.prop_u32("reg", 2); // same property name -> same string offset
        b.end_node();
        b.end_node();
        let dtb = b.finish();
        let off_strings = be32(&dtb, 12) as usize;
        let size_strings = be32(&dtb, 32) as usize;
        let strings = &dtb[off_strings..off_strings + size_strings];
        assert_eq!(strings, b"reg\0", "single deduplicated property name");
    }

    #[test]
    fn virt_fdt_generates_and_is_size_consistent() {
        let dtb = build_virt_fdt(&VirtFdtConfig::default());
        assert_eq!(be32(&dtb, 0), FDT_MAGIC);
        assert_eq!(be32(&dtb, 4) as usize, dtb.len());
        // The strings block must mention the keystone + PCIe nodes' props.
        let off_strings = be32(&dtb, 12) as usize;
        let strings = String::from_utf8_lossy(&dtb[off_strings..]);
        for needed in [
            "compatible",
            "reg",
            "interrupt-map",
            "ranges",
            "msi-parent",
            "msi-controller",
            "arm,msi-base-spi",
            "arm,msi-num-spis",
            "dma-coherent",
        ] {
            assert!(strings.contains(needed), "missing property name {needed}");
        }
    }

    #[test]
    fn virt_fdt_contains_node_names_for_keystone_devices() {
        let dtb = build_virt_fdt(&VirtFdtConfig::default());
        // Node names live verbatim in the structure block.
        let body = String::from_utf8_lossy(&dtb);
        for node in [
            "fw-cfg@9020000",
            "pcie@10000000",
            "intc@8000000",
            "v2m@8080000",
            "pl011@9000000",
            "virtio_mmio@a000000",
        ] {
            assert!(body.contains(node), "missing node {node}");
        }
    }

    #[test]
    fn virt_fdt_advertises_both_64_bit_pcie_mmio_apertures() {
        let dtb = build_virt_fdt(&VirtFdtConfig::default());
        for (space_code, aperture, description) in [
            (
                0x0300_0000u32,
                machine::PCIE_MMIO_64_NON_PREFETCH,
                "non-prefetchable",
            ),
            (
                0x4300_0000u32,
                machine::PCIE_MMIO_64_PREFETCH,
                "prefetchable",
            ),
        ] {
            let cells = [
                space_code,
                (aperture.base >> 32) as u32,
                aperture.base as u32,
                (aperture.base >> 32) as u32,
                aperture.base as u32,
                (aperture.size >> 32) as u32,
                aperture.size as u32,
            ];
            let encoded = cells
                .iter()
                .flat_map(|cell| cell.to_be_bytes())
                .collect::<Vec<_>>();

            assert!(
                dtb.windows(encoded.len()).any(|window| window == encoded),
                "PCIe ranges must expose the {description} high-MMIO aperture"
            );
        }
    }

    #[test]
    #[should_panic(expected = "exceeds GICv3 redistributor window")]
    fn virt_fdt_rejects_too_many_cpus() {
        build_virt_fdt(&VirtFdtConfig {
            cpu_count: 300, // > MAX_CPUS (256) for the Apple hv_gic redist region
            ram_size: 1 << 30,
        });
    }
}
