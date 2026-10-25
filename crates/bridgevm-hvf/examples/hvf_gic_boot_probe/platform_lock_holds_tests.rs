//! The `platform lock holds:` record charges each tick attempt that found the
//! platform lock held to the hold it found, by holder; holds no tick found
//! cost no write; storage and line width are fixed; and every lock site that
//! runs beside the tick holds through it.

use super::*;
use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::pcie;
use bridgevm_hvf::platform_virt::{FlatGuestRam, MmioOp, MmioOutcome};
use std::collections::BTreeMap;
use std::thread;

/// Where these tests map NVMe's BAR0.
pub(crate) const NVME_BAR: u64 = machine::PCIE_MMIO_32.base;

/// Map NVMe's BAR0 at `NVME_BAR` with memory decode on, as firmware does.
pub(crate) fn map_nvme(platform: &mut VirtPlatform) {
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
    let (_, device, function) = pcie::NVME_BDF;
    let function = (u64::from(device) << 15) | (u64::from(function) << 12);
    let cfg = |reg: u16| machine::PCIE_ECAM.base + function + u64::from(reg);
    let command = u64::from(pcie::CMD_MEMORY_SPACE);
    for (address, size, value) in [
        (cfg(pcie::REG_BAR0), 4, NVME_BAR),
        (cfg(pcie::REG_COMMAND_STATUS), 2, command),
    ] {
        let op = MmioOp::Write { size, value };
        assert_eq!(
            platform.on_mmio(address, op, &mut mem),
            MmioOutcome::WriteAck
        );
    }
}

fn nvme_platform() -> Mutex<VirtPlatform> {
    let mut platform = VirtPlatform::new(VirtFdtConfig::default());
    map_nvme(&mut platform);
    Mutex::new(platform)
}

/// The record's fields by name.
pub(crate) fn fields(record: &str) -> BTreeMap<String, u64> {
    let pairs = record.strip_prefix("platform lock holds: ").expect(record);
    let field = |pair: &str| {
        let (name, value) = pair.split_once('=').expect(pair);
        (name.to_string(), value.parse().expect(pair))
    };
    pairs.split(' ').map(field).collect()
}

/// A hold at `site` that `busy` tick attempts found.
fn hold_found(holds: &PlatformHolds, platform: &Mutex<VirtPlatform>, site: HoldSite, busy: u64) {
    let _held = holds.lock(platform, None, 0, site);
    for _ in 0..busy {
        holds.note_busy();
    }
}

/// `holds`, `busy`, `gt_20ms` and `max_us` of `holder`.
fn holder(fields: &BTreeMap<String, u64>, holder: Holder) -> [u64; 4] {
    ["holds", "busy", "gt_20ms", "max_us"]
        .map(|field| fields[&format!("{}_{field}", holder.name())])
}

#[test]
fn a_hold_no_tick_found_costs_no_write() {
    let (holds, platform) = (PlatformHolds::new(), nvme_platform());
    for site in [
        HoldSite::UartPreload,
        HoldSite::PreRun,
        HoldSite::Mmio(NVME_BAR),
        HoldSite::Automation,
    ] {
        for _ in 0..10_000 {
            drop(holds.lock(&platform, None, 0, site));
        }
    }
    drop(holds.try_lock(&platform, HoldSite::Display).unwrap());
    assert_eq!(holds.record(), PlatformHolds::new().record());
    assert!(fields(&holds.record()).values().all(|&value| value == 0));
}

#[test]
fn each_busy_attempt_is_charged_once_to_the_hold_it_found() {
    let (holds, platform) = (PlatformHolds::new(), nvme_platform());
    hold_found(&holds, &platform, HoldSite::PreRun, 2);
    // Between holds: no hold to charge.
    holds.note_busy();
    hold_found(&holds, &platform, HoldSite::Automation, 1);
    hold_found(&holds, &platform, HoldSite::PreRun, 0);
    hold_found(&holds, &platform, HoldSite::PreRun, 3);
    let fields = fields(&holds.record());
    assert_eq!((fields["busy"], fields["unattributed"]), (7, 1));
    assert_eq!(holder(&fields, Holder::PreRun)[..2], [2, 5]);
    assert_eq!(holder(&fields, Holder::Automation)[..2], [1, 1]);
    for other in [Holder::Nvme, Holder::Tpm, Holder::Mmio, Holder::Host] {
        assert_eq!(holder(&fields, other), [0; 4], "{other:?}");
    }
}

#[test]
fn an_mmio_exit_is_charged_to_nvme_the_tpm_or_another_device_by_its_target() {
    let (holds, platform) = (PlatformHolds::new(), nvme_platform());
    // NVMe's registers and its doorbells, where a submission runs its I/O.
    for ipa in [NVME_BAR, NVME_BAR + 0x1000] {
        hold_found(&holds, &platform, HoldSite::Mmio(ipa), 1);
    }
    for ipa in [machine::TPM_TIS.base, machine::TPM_PPI.base] {
        hold_found(&holds, &platform, HoldSite::Mmio(ipa), 1);
    }
    let other = [
        machine::UART.base,
        machine::FW_CFG.base,
        NVME_BAR + u64::from(pcie::NVME_BAR0_SIZE),
    ];
    for ipa in other {
        hold_found(&holds, &platform, HoldSite::Mmio(ipa), 1);
    }
    // Before firmware maps NVMe's BAR, its future address is no NVMe exit.
    let unmapped = Mutex::new(VirtPlatform::new(VirtFdtConfig::default()));
    hold_found(&holds, &unmapped, HoldSite::Mmio(NVME_BAR), 1);
    hold_found(&holds, &platform, HoldSite::UartPreload, 1);
    {
        let _held = holds.try_lock(&platform, HoldSite::Display).unwrap();
        holds.note_busy();
    }
    let fields = fields(&holds.record());
    let charged = [Holder::Nvme, Holder::Tpm, Holder::Mmio, Holder::Host]
        .map(|charged| holder(&fields, charged)[..2].to_vec());
    assert_eq!(charged, [[2, 2], [2, 2], [4, 4], [2, 2]].map(Vec::from));
    assert_eq!((fields["busy"], fields["unattributed"]), (10, 0));
}

#[test]
fn a_hold_over_20_ms_is_long_and_the_longest_is_kept() {
    let holds = PlatformHolds::new();
    holds.charge(Holder::Nvme, 1, Duration::from_millis(20));
    holds.charge(Holder::Nvme, 2, Duration::from_micros(20_001));
    holds.charge(Holder::Nvme, 79, Duration::from_millis(394));
    holds.charge(Holder::Nvme, 1, Duration::from_micros(150));
    let nvme = holder(&fields(&holds.record()), Holder::Nvme);
    assert_eq!(nvme, [4, 83, 2, 394_000]);
    // A real hold that a tick found and that outlasted 20 ms.
    let platform = nvme_platform();
    {
        let _held = holds.lock(&platform, None, 0, HoldSite::Automation);
        holds.note_busy();
        thread::sleep(Duration::from_millis(25));
    }
    let [charged, busy, long, max_us] = holder(&fields(&holds.record()), Holder::Automation);
    assert_eq!([charged, busy, long], [1, 1, 1]);
    assert!(max_us >= 25_000, "{max_us} µs");
}

#[test]
fn the_record_prints_every_holder_in_a_fixed_order() {
    let holds = PlatformHolds::new();
    for _ in 0..4 {
        holds.note_busy();
    }
    holds.charge(Holder::Tpm, 2, Duration::from_micros(30_500));
    holds.charge(Holder::Host, 1, Duration::from_micros(7));
    assert_eq!(
        holds.record(),
        "platform lock holds: busy=4 unattributed=1 \
         prerun_holds=0 prerun_busy=0 prerun_gt_20ms=0 prerun_max_us=0 \
         nvme_holds=0 nvme_busy=0 nvme_gt_20ms=0 nvme_max_us=0 \
         tpm_holds=1 tpm_busy=2 tpm_gt_20ms=1 tpm_max_us=30500 \
         mmio_holds=0 mmio_busy=0 mmio_gt_20ms=0 mmio_max_us=0 \
         automation_holds=0 automation_busy=0 automation_gt_20ms=0 automation_max_us=0 \
         host_holds=1 host_busy=1 host_gt_20ms=0 host_max_us=7"
    );
}

#[test]
fn storage_and_the_line_are_bounded_whatever_the_holds() {
    // The tick's count and four counters per holder, however many holds.
    assert_eq!(
        std::mem::size_of::<PlatformHolds>(),
        8 * (1 + 4 * Holder::ALL.len())
    );
    let holds = PlatformHolds::new();
    let platform = nvme_platform();
    for _ in 0..10_000 {
        hold_found(&holds, &platform, HoldSite::PreRun, 1);
    }
    let fields = fields(&holds.record());
    assert_eq!(holder(&fields, Holder::PreRun)[..2], [10_000, 10_000]);
    // Every value at u64::MAX: 26 fields of at most 20 digits, under 1 KiB.
    let widest = PlatformHolds::new();
    widest.busy.store(u64::MAX, SeqCst);
    for counts in &widest.holders {
        for counter in [&counts.holds, &counts.busy, &counts.long, &counts.max_us] {
            counter.store(u64::MAX, Relaxed);
        }
    }
    let names = ["busy", "unattributed"]
        .into_iter()
        .map(str::len)
        .sum::<usize>()
        + Holder::ALL
            .iter()
            .map(|holder| 4 * holder.name().len() + "_holds_busy_gt_20ms_max_us".len())
            .sum::<usize>();
    let bound = "platform lock holds:".len() + names + 26 * (" =".len() + 20);
    assert_eq!(bound, 888);
    let record = widest.record();
    assert!(
        record.len() <= bound,
        "{} > {bound}: {record}",
        record.len()
    );
    assert!(record.contains(" unattributed=0 "), "saturates: {record}");
}

#[test]
fn every_lock_site_beside_the_tick_holds_through_the_record() {
    let count = |text: &str, needle: &str| text.matches(needle).count();
    let run = include_str!("probe_runtime.rs");
    for site in [
        "HoldSite::UartPreload",
        "HoldSite::Mmio(ipa)",
        "HoldSite::Automation",
    ] {
        assert_eq!(count(run, site), 1, "{site}");
    }
    assert_eq!(count(run, "lock_platform("), 3);
    let secondary = include_str!("secondary_vcpu.rs");
    assert_eq!(count(secondary, "lock_platform("), 1);
    assert_eq!(count(secondary, "HoldSite::Mmio(ipa)"), 1);
    let prepare = include_str!("interrupt_delivery/prepare.rs");
    assert_eq!(count(prepare, "lock_platform("), 2);
    assert_eq!(count(prepare, "HoldSite::PreRun"), 2);
    let display = include_str!("ramfb_display_thread.rs");
    let copy = "match try_lock_platform(&self.platform, HoldSite::Display) {";
    assert_eq!(
        (count(display, copy), count(display, ".try_lock()")),
        (1, 0)
    );
    let tick = include_str!("hda_service_tick.rs");
    let port = "service_once(&self.platform, &PLATFORM_HOLDS, &mut self.memory, now)";
    assert_eq!(count(tick, port), 1);
    let this = include_str!("platform_lock_holds.rs");
    assert_eq!(
        count(this, "PLATFORM_HOLDS.lock(platform, smp_trace, cpu, site)"),
        1
    );
    assert_eq!(count(this, "PLATFORM_HOLDS.try_lock(platform, site)"), 1);
    // The other lockers run while no tick can: before the tick starts (the
    // vblank wake state, the display's device check and timing install, the
    // tick's own start), across a reset the tick is quiesced for, and in the
    // final report after the tick is joined.
    for (text, needle, sites) in [
        (run, ".lock()", 1),
        (display, "platform.lock()", 1),
        (
            include_str!("ramfb_display_timing.rs"),
            "platform.lock()",
            1,
        ),
        (tick, "platform.lock()", 1),
        (
            include_str!("probe_runtime/system_reset_failure.rs"),
            "platform.lock()",
            1,
        ),
        (include_str!("final_report.rs"), "$platform.lock()", 1),
    ] {
        assert_eq!(count(text, needle), sites, "{needle}");
    }
}
