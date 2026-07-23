//! Serial milestone scanning and boot timer telemetry.

use crate::*;

pub(crate) fn maybe_write_file(path_env: &str, bytes: &[u8], description: &str) {
    if let Ok(path) = std::env::var(path_env) {
        let label = format!("{description} written");
        write_named_bytes(&path, bytes, &label);
    }
}

pub(crate) fn symbol_lines(serial: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(serial)
        .lines()
        .filter(|line| line.starts_with("add-symbol-file "))
        .map(str::to_owned)
        .collect()
}

pub(crate) fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[derive(Default)]
pub(crate) struct SerialStopScans {
    pub(crate) shell_prompt: IncrementalSerialScan,
    pub(crate) shell_short_prompt: IncrementalSerialScan,
    pub(crate) linux_boot_cpu: IncrementalSerialScan,
    pub(crate) linux_version: IncrementalSerialScan,
    pub(crate) linux_panic: IncrementalSerialScan,
}

#[derive(Default)]
pub(crate) struct IncrementalSerialScan {
    pub(crate) scanned_len: usize,
    pub(crate) found: bool,
}

impl IncrementalSerialScan {
    pub(crate) fn contains_new(&mut self, haystack: &[u8], needle: &[u8]) -> bool {
        if self.found {
            return true;
        }
        if needle.is_empty() {
            self.found = true;
            return true;
        }
        let overlap = needle.len().saturating_sub(1);
        let start = self.scanned_len.saturating_sub(overlap).min(haystack.len());
        self.scanned_len = haystack.len();
        self.found = contains_bytes(&haystack[start..], needle);
        self.found
    }
}

pub(crate) struct BootTimer {
    pub(crate) enabled: bool,
    pub(crate) start: Instant,
    pub(crate) next_display_sample: Instant,
    pub(crate) display_interval: Duration,
    pub(crate) expected_desktop_checksum: Option<u64>,
    pub(crate) desktop_agent: bool,
    pub(crate) desktop_reached: bool,
    pub(crate) milestones: Vec<BootTimerSerialMilestone>,
}

pub(crate) struct BootTimerSerialMilestone {
    pub(crate) name: &'static str,
    pub(crate) needle: &'static [u8],
    pub(crate) scan: IncrementalSerialScan,
    pub(crate) emitted: bool,
}

impl BootTimer {
    const MAX_SERVICE_WAKE_INTERVAL: Duration = Duration::from_millis(250);
    pub(crate) fn from_env() -> Self {
        if !env_flag("BRIDGEVM_BOOT_TIMER") {
            return Self::disabled();
        }
        let now = Instant::now();
        let display_interval =
            Duration::from_millis(env_u64("BRIDGEVM_BOOT_TIMER_RAMFB_MS", 1000).clamp(100, 60_000));
        let expected_desktop_checksum = env_optional_u64("BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64");
        let desktop_agent = env_flag("BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT");
        println!(
            "BOOT_TIMER start ramfb_sample_ms={} desktop_checksum={} desktop_agent={}",
            display_interval.as_millis(),
            format_optional_u64_hex(expected_desktop_checksum),
            desktop_agent
        );
        let mut timer = Self::new(
            true,
            now,
            display_interval,
            expected_desktop_checksum,
            boot_timer_default_milestones(),
        );
        timer.desktop_agent = desktop_agent;
        timer
    }
    pub(crate) fn disabled() -> Self {
        let now = Instant::now();
        Self::new(false, now, Duration::from_secs(1), None, Vec::new())
    }
    pub(crate) fn new(
        enabled: bool,
        start: Instant,
        display_interval: Duration,
        expected_desktop_checksum: Option<u64>,
        milestones: Vec<BootTimerSerialMilestone>,
    ) -> Self {
        Self {
            enabled,
            start,
            next_display_sample: start,
            display_interval,
            expected_desktop_checksum,
            desktop_agent: false,
            desktop_reached: false,
            milestones,
        }
    }
    pub(crate) fn tick(
        &mut self,
        platform: &VirtPlatform,
        mem: &dyn GuestMemoryMut,
        exit: u64,
        cpu0_pc: u64,
    ) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.start);
        self.scan_serial(platform.uart_output(), elapsed, exit);
        if now >= self.next_display_sample {
            self.next_display_sample = now + self.display_interval;
            self.sample_display(platform, mem, elapsed, exit, cpu0_pc);
        }
    }
    pub(crate) fn scan_serial(&mut self, serial: &[u8], elapsed: Duration, exit: u64) {
        for milestone in &mut self.milestones {
            if !milestone.emitted && milestone.scan.contains_new(serial, milestone.needle) {
                milestone.emitted = true;
                println!(
                    "BOOT_TIMER milestone name={} source=serial elapsed_ms={} exit={}",
                    milestone.name,
                    elapsed.as_millis(),
                    exit
                );
            }
        }
    }
    pub(crate) fn sample_display(
        &mut self,
        platform: &VirtPlatform,
        mem: &dyn GuestMemoryMut,
        elapsed: Duration,
        exit: u64,
        cpu0_pc: u64,
    ) {
        match boot_timer_display_summary(platform, mem) {
            Ok((source, summary)) => {
                let desktop_match =
                    self.expected_desktop_checksum == Some(summary.checksum64);
                if desktop_match && !self.desktop_reached {
                    self.desktop_reached = true;
                    println!(
                        "BOOT_TIMER milestone name=desktop source={} elapsed_ms={} exit={} checksum64={:#018x}",
                        source,
                        elapsed.as_millis(),
                        exit,
                        summary.checksum64
                    );
                }
                println!(
                    "BOOT_TIMER ramfb source={} state=captured elapsed_ms={} exit={} pc={:#x} checksum64={:#018x} nonzero_pixels={} unique_colors={} desktop_match={}",
                    source,
                    elapsed.as_millis(),
                    exit,
                    cpu0_pc,
                    summary.checksum64,
                    summary.nonzero_pixels,
                    summary.unique_colors,
                    desktop_match
                );
            }
            Err(BootTimerDisplayState::Inactive) => println!(
                "BOOT_TIMER ramfb source=none state=inactive elapsed_ms={} exit={} pc={:#x}",
                elapsed.as_millis(),
                exit,
                cpu0_pc
            ),
            Err(BootTimerDisplayState::Unavailable { source, error }) => println!(
                "BOOT_TIMER ramfb source={} state=unavailable elapsed_ms={} exit={} pc={:#x} error={:?}",
                source,
                elapsed.as_millis(),
                exit,
                cpu0_pc,
                error
            ),
        }
    }
    pub(crate) fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
    pub(crate) fn observe_agent_ready(&mut self, now: Instant, exit: u64) {
        if !self.enabled || !self.desktop_agent || self.desktop_reached {
            return;
        }
        self.desktop_reached = true;
        println!(
            "BOOT_TIMER milestone name=desktop source=agent elapsed_ms={} exit={}",
            now.saturating_duration_since(self.start).as_millis(),
            exit
        );
    }
    pub(crate) fn print_summary(
        &self,
        elapsed: Duration,
        cpu0_exits: u64,
        secondary_exits: &[(u64, u64)],
    ) {
        if !self.enabled {
            return;
        }
        println!(
            "BOOT_TIMER summary elapsed_ms={} desktop_reached={} milestones={}/{}",
            elapsed.as_millis(),
            self.desktop_reached,
            self.milestones
                .iter()
                .filter(|milestone| milestone.emitted)
                .count(),
            self.milestones.len()
        );
        print_boot_timer_vcpu_rate(0, cpu0_exits, elapsed);
        for (cpu, exits) in secondary_exits {
            print_boot_timer_vcpu_rate(*cpu, *exits, elapsed);
        }
    }
    pub(crate) fn service_wake_interval(&self) -> Option<Duration> {
        self.enabled
            .then(|| self.display_interval.min(Self::MAX_SERVICE_WAKE_INTERVAL))
    }
}

impl BootTimerSerialMilestone {
    pub(crate) fn new(name: &'static str, needle: &'static [u8]) -> Self {
        Self {
            name,
            needle,
            scan: IncrementalSerialScan::default(),
            emitted: false,
        }
    }
}

pub(crate) enum BootTimerDisplayState {
    Inactive,
    Unavailable {
        source: &'static str,
        error: RamfbSnapshotError,
    },
}

pub(crate) fn boot_timer_default_milestones() -> Vec<BootTimerSerialMilestone> {
    [
        ("edk2-bds", b"BdsDxe: starting" as &[u8]),
        ("cdboot-prompt", b"Press any key to boot from CD or DVD"),
        ("uefi-shell", b"UEFI Interactive Shell"),
        ("linux-early", b"Booting Linux on physical CPU"),
        ("linux-version", b"Linux version"),
        ("windows-boot-manager", b"Windows Boot Manager"),
        ("windows-kernel", b"Windows Boot Loader"),
        ("bvagent", b"BVAGENT"),
    ]
    .into_iter()
    .map(|(name, needle)| BootTimerSerialMilestone::new(name, needle))
    .collect()
}

pub(crate) fn boot_timer_display_summary(
    platform: &VirtPlatform,
    mem: &dyn GuestMemoryMut,
) -> Result<(&'static str, RamfbSnapshotSummary), BootTimerDisplayState> {
    if let Some(scanout) = platform.virtio_gpu_scanout() {
        let config = RamfbConfig {
            addr: 1,
            fourcc: scanout.fourcc,
            flags: 0,
            width: scanout.width,
            height: scanout.height,
            stride: scanout.stride,
        };
        return RamfbSnapshot::summarize_xrgb8888_bytes(config, scanout.bytes)
            .map(|summary| ("virtio-gpu", summary))
            .map_err(|error| BootTimerDisplayState::Unavailable {
                source: "virtio-gpu",
                error,
            });
    }
    let Some(config) = platform.ramfb_config() else {
        return Err(BootTimerDisplayState::Inactive);
    };
    RamfbSnapshot::read_from(mem, config)
        .map(|snapshot| ("ramfb", snapshot.summary))
        .map_err(|error| BootTimerDisplayState::Unavailable {
            source: "ramfb",
            error,
        })
}

pub(crate) fn print_boot_timer_vcpu_rate(cpu: u64, exits: u64, elapsed: Duration) {
    let seconds = elapsed.as_secs_f64();
    let exits_per_sec = if seconds > 0.0 {
        exits as f64 / seconds
    } else {
        0.0
    };
    println!(
        "BOOT_TIMER vcpu cpu={} exits={} exits_per_sec={:.2}",
        cpu, exits, exits_per_sec
    );
}

#[cfg(test)]
mod boot_timer_tests {
    use super::*;

    #[test]
    fn flag_parser_trims_and_ignores_ascii_case() {
        assert_eq!(parse_flag(" False "), Some(false));
        assert_eq!(parse_flag("\tON\n"), Some(true));
        assert_eq!(parse_flag("unexpected"), None);
    }

    #[test]
    fn boot_timer_serial_milestone_handles_split_marker_once() {
        let now = Instant::now();
        let mut timer = BootTimer::new(
            true,
            now,
            Duration::from_secs(1),
            None,
            vec![BootTimerSerialMilestone::new("test", b"Boot0001")],
        );

        timer.scan_serial(b"BdsDxe: starting Boo", Duration::ZERO, 1);
        assert!(!timer.milestones[0].emitted);

        timer.scan_serial(b"BdsDxe: starting Boot0001", Duration::ZERO, 2);
        assert!(timer.milestones[0].emitted);

        timer.scan_serial(b"BdsDxe: starting Boot0001", Duration::ZERO, 3);
        assert!(timer.milestones[0].emitted);
    }

    #[test]
    fn disabled_boot_timer_has_no_milestone_work() {
        let timer = BootTimer::disabled();

        assert!(!timer.enabled);
        assert!(timer.milestones.is_empty());
    }

    #[test]
    fn agent_oracle_marks_desktop_without_exact_frame_checksum() {
        let now = Instant::now();
        let mut timer = BootTimer::new(true, now, Duration::from_secs(1), None, Vec::new());
        timer.desktop_agent = true;

        timer.observe_agent_ready(now + Duration::from_millis(25), 7);

        assert!(timer.desktop_reached);
    }

    #[test]
    fn service_wake_honors_short_display_interval_without_slowing_default_polling() {
        let now = Instant::now();
        let short = BootTimer::new(true, now, Duration::from_millis(100), None, Vec::new());
        let long = BootTimer::new(true, now, Duration::from_secs(1), None, Vec::new());

        assert_eq!(
            short.service_wake_interval(),
            Some(Duration::from_millis(100))
        );
        assert_eq!(
            long.service_wake_interval(),
            Some(Duration::from_millis(250))
        );
        assert_eq!(BootTimer::disabled().service_wake_interval(), None);
    }
}

#[derive(Clone, Copy)]
pub(crate) enum DrainLocation {
    PreRun,
    DataAbort,
}
