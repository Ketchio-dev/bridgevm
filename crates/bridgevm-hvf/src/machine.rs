//! `virt`-compatible machine model — the single source of truth for the
//! BridgeVM HVF "QEMU virt contract" path (Path A, see
//! `docs/hvf-windows-engine-strategy.md`).
//!
//! Every address and interrupt number here is transcribed from the authoritative
//! QEMU 11.0.1 `virt` (GICv3) device tree dumped in
//! `docs/reference/qemu-virt-aarch64-gicv3.dts`. The point of reproducing the
//! exact QEMU layout is so the stock ArmVirtQemu firmware boots unmodified and
//! the guest sees a platform bit-identical to the QEMU stack that already
//! installs Windows 11 ARM. The legacy probe harness in `lib.rs` uses a different,
//! colliding map (`docs/hvf-windows-platform-contract-gap.md`); new platform code
//! must build on this module instead.
//!
//! Pure data + logic — no Hypervisor.framework calls, so it builds and tests on
//! any host.

/// A guest-physical address range `[base, base + size)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub base: u64,
    pub size: u64,
}

impl Region {
    pub const fn new(base: u64, size: u64) -> Self {
        Self { base, size }
    }
    /// Exclusive end address.
    pub const fn end(&self) -> u64 {
        self.base + self.size
    }
    pub const fn contains(&self, addr: u64) -> bool {
        addr >= self.base && addr < self.end()
    }
    /// True if the two ranges share any address (zero-size ranges never overlap).
    pub const fn overlaps(&self, other: &Region) -> bool {
        self.size != 0 && other.size != 0 && self.base < other.end() && other.base < self.end()
    }
}

// ---- Memory map (QEMU `virt`, GICv3) ---------------------------------------

/// pflash: two 64 MiB banks (code + vars) at the very bottom of the address space.
pub const FLASH: Region = Region::new(0x0, 0x0800_0000);
/// pflash bank 0 — firmware code (`edk2-aarch64-code.fd`), read-only.
pub const FLASH_CODE: Region = Region::new(0x0, 0x0400_0000);
/// pflash bank 1 — UEFI variable store (`edk2-arm-vars.fd`), writable.
pub const FLASH_VARS: Region = Region::new(0x0400_0000, 0x0400_0000);

/// GICv3 distributor (`0x10000` @ `0x10000` alignment — matches both QEMU virt
/// and Apple `hv_gic`).
pub const GIC_DIST: Region = Region::new(0x0800_0000, 0x0001_0000);
/// GICv3 ITS / MSI region (QEMU reference placement; reserved — not yet wired to
/// Apple `hv_gic`'s MSI, so currently omitted from the generated DTB).
pub const GIC_ITS: Region = Region::new(0x0808_0000, 0x0002_0000);
/// GICv3 redistributor window. Sized/placed for **Apple `hv_gic`**, which requires
/// a 32 MiB (`0x0200_0000`) redistributor region — larger than QEMU virt's
/// `0x080A_0000`/`0xF6_0000` slot, so it is relocated into the free gap between the
/// virtio-mmio block and the PCIe MMIO window. The firmware reads this from the
/// generated DTB, so the placement only has to be internally consistent (the
/// no-overlap validator enforces that).
pub const GIC_REDIST: Region = Region::new(0x0C00_0000, 0x0200_0000);

/// PL011 UART (firmware/OS serial console).
pub const UART: Region = Region::new(0x0900_0000, 0x1000);
/// PL031 real-time clock.
pub const RTC: Region = Region::new(0x0901_0000, 0x1000);
/// `fw_cfg` — the ACPI/SMBIOS/boot-order/kernel handoff keystone (see [`crate::fwcfg`]).
pub const FW_CFG: Region = Region::new(0x0902_0000, 0x18);
/// PL061 GPIO controller.
pub const GPIO: Region = Region::new(0x0903_0000, 0x1000);

/// Size of a single virtio-mmio transport slot.
pub const VIRTIO_MMIO_SLOT_SIZE: u64 = 0x200;
/// Number of virtio-mmio slots QEMU `virt` exposes.
pub const VIRTIO_MMIO_COUNT: u64 = 32;
/// virtio-mmio transport array.
pub const VIRTIO_MMIO: Region =
    Region::new(0x0A00_0000, VIRTIO_MMIO_SLOT_SIZE * VIRTIO_MMIO_COUNT);

/// PCIe ECAM config space (`pci-host-ecam-generic`), buses 0..=0xff.
pub const PCIE_ECAM: Region = Region::new(0x40_1000_0000, 0x1000_0000);
/// PCIe port I/O window.
pub const PCIE_PIO: Region = Region::new(0x3EFF_0000, 0x1_0000);
/// PCIe 32-bit MMIO window (BAR space below 4 GiB).
pub const PCIE_MMIO_32: Region = Region::new(0x1000_0000, 0x2EFF_0000);
/// PCIe 64-bit MMIO window (high BAR space).
pub const PCIE_MMIO_64: Region = Region::new(0x80_0000_0000, 0x80_0000_0000);

/// Base of system RAM. The only address the legacy probe map already gets right.
pub const RAM_BASE: u64 = 0x4000_0000;

// ---- Interrupt map ----------------------------------------------------------

/// GIC SPI `n` maps to INTID `n + 32`.
pub const SPI_OFFSET: u32 = 32;
/// Convert a GIC SPI number to its absolute INTID.
pub const fn spi_to_intid(spi: u32) -> u32 {
    spi + SPI_OFFSET
}

/// PL011 UART → SPI 1.
pub const SPI_UART: u32 = 1;
/// PL031 RTC → SPI 2.
pub const SPI_RTC: u32 = 2;
/// PCIe legacy INTA → SPI 3 (INTB=4, INTC=5, INTD=6, swizzled per device).
pub const SPI_PCIE_INTA: u32 = 3;
/// PL061 GPIO → SPI 7.
pub const SPI_GPIO: u32 = 7;
/// First virtio-mmio slot → SPI 16; slot `i` → SPI `16 + i`.
pub const SPI_VIRTIO_MMIO_BASE: u32 = 16;

/// SPI for virtio-mmio slot `i`.
pub const fn virtio_mmio_spi(i: u32) -> u32 {
    SPI_VIRTIO_MMIO_BASE + i
}

// Arch timer / PMU per-CPU PPIs (device-tree interrupt type 1).
/// Secure EL1 physical timer.
pub const PPI_TIMER_SECURE: u32 = 13;
/// Non-secure EL1 physical timer.
pub const PPI_TIMER_NONSEC: u32 = 14;
/// EL1 virtual timer.
pub const PPI_TIMER_VIRT: u32 = 11;
/// EL2 hypervisor timer.
pub const PPI_TIMER_HYP: u32 = 10;
/// PMU overflow interrupt.
pub const PPI_PMU: u32 = 7;

// ---- GICv3 sizing -----------------------------------------------------------

/// Per-CPU GICv3 redistributor stride (RD_base + SGI_base = 2 × 64 KiB).
pub const GICV3_REDIST_STRIDE: u64 = 0x2_0000;
/// Maximum CPUs the [`GIC_REDIST`] window can host.
pub const MAX_CPUS: u64 = GIC_REDIST.size / GICV3_REDIST_STRIDE;

/// Whether `cpu_count` redistributors fit in the redistributor window.
pub const fn redist_fits(cpu_count: u64) -> bool {
    cpu_count <= MAX_CPUS
}

// ---- Helpers & validation ---------------------------------------------------

/// MMIO region for virtio-mmio slot `i` (panics if `i >= VIRTIO_MMIO_COUNT`).
pub fn virtio_mmio_slot(i: u64) -> Region {
    assert!(i < VIRTIO_MMIO_COUNT, "virtio-mmio slot {i} out of range");
    Region::new(
        VIRTIO_MMIO.base + i * VIRTIO_MMIO_SLOT_SIZE,
        VIRTIO_MMIO_SLOT_SIZE,
    )
}

/// The fixed MMIO device regions, in ascending base order. (RAM and the PCIe
/// 64-bit window are intentionally excluded — RAM is system memory and the
/// 64-bit window lives far above the device block.)
pub fn mmio_regions() -> [(&'static str, Region); 13] {
    [
        ("flash-code", FLASH_CODE),
        ("flash-vars", FLASH_VARS),
        ("gic-dist", GIC_DIST),
        ("gic-its", GIC_ITS),
        ("gic-redist", GIC_REDIST),
        ("uart", UART),
        ("rtc", RTC),
        ("fw-cfg", FW_CFG),
        ("gpio", GPIO),
        ("virtio-mmio", VIRTIO_MMIO),
        ("pcie-mmio-32", PCIE_MMIO_32),
        ("pcie-pio", PCIE_PIO),
        ("pcie-ecam", PCIE_ECAM),
    ]
}

/// First overlapping pair among the fixed MMIO regions, if any. A clean machine
/// model returns `None`; this guards against the address collisions documented
/// in `docs/hvf-windows-platform-contract-gap.md`.
pub fn first_overlap() -> Option<(&'static str, &'static str)> {
    let regions = mmio_regions();
    for i in 0..regions.len() {
        for j in (i + 1)..regions.len() {
            if regions[i].1.overlaps(&regions[j].1) {
                return Some((regions[i].0, regions[j].0));
            }
        }
    }
    None
}

/// Name of the fixed MMIO device a guest-physical address falls in, if any.
pub fn device_at(addr: u64) -> Option<&'static str> {
    mmio_regions()
        .into_iter()
        .find(|(_, r)| r.contains(addr))
        .map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bases_match_the_authoritative_contract() {
        assert_eq!(FLASH_CODE.base, 0x0);
        assert_eq!(GIC_DIST.base, 0x0800_0000);
        assert_eq!(GIC_ITS.base, 0x0808_0000);
        // Redistributor relocated for Apple hv_gic's 32 MiB region (see const docs).
        assert_eq!(GIC_REDIST.base, 0x0C00_0000);
        assert_eq!(GIC_REDIST.size, 0x0200_0000);
        assert_eq!(UART.base, 0x0900_0000);
        assert_eq!(RTC.base, 0x0901_0000);
        assert_eq!(FW_CFG.base, 0x0902_0000);
        assert_eq!(GPIO.base, 0x0903_0000);
        assert_eq!(VIRTIO_MMIO.base, 0x0A00_0000);
        assert_eq!(PCIE_MMIO_32.base, 0x1000_0000);
        assert_eq!(PCIE_ECAM.base, 0x40_1000_0000);
        assert_eq!(RAM_BASE, 0x4000_0000);
    }

    #[test]
    fn fw_cfg_region_agrees_with_the_device_model() {
        // The machine map and the fw_cfg device must not drift apart.
        assert_eq!(FW_CFG.base, crate::fwcfg::FW_CFG_MMIO_BASE);
        assert_eq!(FW_CFG.size, crate::fwcfg::FW_CFG_MMIO_SIZE);
    }

    #[test]
    fn flash_is_two_64mib_banks_below_the_gic() {
        assert_eq!(FLASH.size, 0x0800_0000);
        assert_eq!(FLASH_CODE.size, 0x0400_0000);
        assert_eq!(FLASH_VARS.base, 0x0400_0000);
        // Flash ends exactly where the GIC distributor begins (adjacent, no gap).
        assert_eq!(FLASH.end(), GIC_DIST.base);
    }

    #[test]
    fn fixed_mmio_regions_do_not_overlap() {
        assert_eq!(first_overlap(), None, "MMIO regions must not collide");
    }

    #[test]
    fn virtio_mmio_slots_are_contiguous_and_irq_mapped() {
        assert_eq!(virtio_mmio_slot(0).base, 0x0A00_0000);
        assert_eq!(virtio_mmio_slot(1).base, 0x0A00_0200);
        assert_eq!(virtio_mmio_slot(31).base, 0x0A00_3E00);
        assert_eq!(virtio_mmio_slot(31).end(), VIRTIO_MMIO.end());
        // Slot 0 → SPI 16 → INTID 48 (matches the reference DTS).
        assert_eq!(virtio_mmio_spi(0), 16);
        assert_eq!(spi_to_intid(virtio_mmio_spi(0)), 48);
        assert_eq!(spi_to_intid(virtio_mmio_spi(31)), 79);
    }

    #[test]
    fn spi_to_intid_matches_known_lines() {
        assert_eq!(spi_to_intid(SPI_UART), 33);
        assert_eq!(spi_to_intid(SPI_RTC), 34);
        assert_eq!(spi_to_intid(SPI_PCIE_INTA), 35);
        assert_eq!(spi_to_intid(SPI_GPIO), 39);
    }

    #[test]
    fn redistributor_window_sizes_match_gicv3() {
        // Apple hv_gic region 0x2000000 / 0x20000 per-CPU stride = 256 CPUs.
        assert_eq!(MAX_CPUS, 256);
        assert!(redist_fits(1));
        assert!(redist_fits(256));
        assert!(!redist_fits(257));
    }

    #[test]
    fn device_lookup_resolves_addresses() {
        assert_eq!(device_at(UART.base), Some("uart"));
        assert_eq!(device_at(FW_CFG.base + 0x8), Some("fw-cfg"));
        assert_eq!(device_at(0x0A00_3E10), Some("virtio-mmio"));
        // A hole between GPIO and the virtio block resolves to nothing.
        assert_eq!(device_at(0x0905_0000), None);
    }

    #[test]
    fn legacy_device_window_collides_with_pcie_mmio_32() {
        // The legacy probe map put its device window at 0x1000_0000, which is the
        // PCIe 32-bit MMIO window here — a documented reason that map cannot host
        // stock firmware. Lock the PCIe base so a future edit cannot silently
        // reintroduce the overlap.
        const LEGACY_DEVICE_MMIO_IPA: u64 = 0x1000_0000;
        assert_eq!(PCIE_MMIO_32.base, LEGACY_DEVICE_MMIO_IPA);
    }
}
