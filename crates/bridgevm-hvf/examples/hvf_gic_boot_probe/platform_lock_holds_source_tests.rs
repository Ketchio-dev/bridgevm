//! The split automation scopes retain instrumentation at every actual acquisition.

#[path = "platform_lock_holds_sources.rs"]
mod sources;

#[test]
fn every_lock_site_beside_the_tick_holds_through_the_record() {
    let count = |text: &str, needle: &str| text.matches(needle).count();
    let run_source = sources::runtime();
    let run = run_source.as_str();
    for site in [
        "HoldSite::UartPreload",
        "HoldSite::Mmio(ipa)",
        "HoldSite::Automation",
    ] {
        // An MMIO exit holds again when an NVMe doorbell queued reads or
        // writes: around the doorbell, and after the unlocked transfer when
        // the vCPU's worker finished it within the wait. A virtio-gpu kick
        // that queued a 2D copy holds again after the unlocked copy. A pause
        // for a full Mac disk holds for its report and for the retry on RESUME.
        let sites = match site {
            "HoldSite::Automation" => 5,
            "HoldSite::Mmio(ipa)" => 3,
            _ => 1,
        };
        assert_eq!(count(run, site), sites, "{site}");
    }
    assert_eq!(count(run, "lock_platform("), 9);
    assert_eq!(count(run, "nvme_io_worker.run(io, &"), 1);
    assert_eq!(count(run, "drain_stats.run_gpu_copies("), 1);
    let secondary = include_str!("secondary_vcpu.rs");
    assert_eq!(count(secondary, "lock_platform("), 3);
    assert_eq!(count(secondary, "HoldSite::Mmio(ipa)"), 3);
    assert_eq!(count(secondary, "nvme_io_worker.run(io, &"), 1);
    assert_eq!(count(secondary, "drain_stats.run_gpu_copies("), 1);
    // The copy loop locks only through the caller's `lock_platform`.
    let gpu = include_str!("gpu_unlocked_copy.rs");
    assert_eq!(count(gpu, "lock();"), 1);
    assert_eq!(count(gpu, ".lock()"), 0);
    let prepare = include_str!("interrupt_delivery/prepare.rs");
    assert_eq!(count(prepare, "lock_platform("), 2);
    assert_eq!(count(prepare, "HoldSite::PreRun"), 2);
    let display = include_str!("ramfb_display_thread.rs");
    let copy = "match try_lock_platform(&self.platform, HoldSite::Display) {";
    assert_eq!(
        (count(display, copy), count(display, ".try_lock()")),
        (1, 0)
    );
    // The agent service thread's one acquisition, its drain of the guest's bytes.
    let service = include_str!("agent_console/service_thread.rs");
    let drain = "crate::lock_platform(self, None, 0, crate::HoldSite::AgentService)";
    assert_eq!(
        (count(service, drain), count(service, "lock_platform(")),
        (1, 1)
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
