use super::*;

pub(crate) struct ProbeConfig {
    pub(crate) media: VirtBootMediaConfig,
    pub(crate) smp_cpus: u64,
    pub(crate) swtpm_data_socket: Option<PathBuf>,
    pub(crate) swtpm_control_socket: Option<PathBuf>,
    pub(crate) platform_cfg: VirtPlatformConfig,
    pub(crate) ram_size: usize,
    pub(crate) watchdog_ms: u64,
    pub(crate) watchdog_enabled: bool,
    pub(crate) trace_fwcfg: bool,
    pub(crate) trace_msix: bool,
    pub(crate) trace_spi: bool,
    pub(crate) trace_run_loop: bool,
    pub(crate) trace_xhci_bringup: bool,
    pub(crate) smp_trace_enabled: bool,
    pub(crate) stop_on_linux: bool,
}

impl ProbeConfig {
    pub(crate) fn from_env() -> Self {
        let media = VirtBootMediaConfig::from_probe_env();
        let smp_cpus = env_u64("BRIDGEVM_SMP_CPUS", 1).clamp(1, machine::MAX_CPUS);
        let smp_cpus = if machine::redist_fits(smp_cpus) {
            smp_cpus
        } else {
            1
        };
        let swtpm_data_socket = std::env::var_os("BRIDGEVM_SWTPM_DATA_SOCKET").map(PathBuf::from);
        let swtpm_control_socket =
            std::env::var_os("BRIDGEVM_SWTPM_CONTROL_SOCKET").map(PathBuf::from);
        let mut platform_devices = media.platform_devices;
        platform_devices.tpm_tis_present = swtpm_data_socket.is_some();
        let platform_cfg = VirtPlatformConfig {
            fdt: VirtFdtConfig {
                cpu_count: smp_cpus,
                ram_size: media.ram_size,
            },
            devices: platform_devices,
        };
        let ram_size = usize::try_from(media.ram_size).expect("guest RAM size does not fit usize");
        assert!(
            ram_size >= 128 * 1024 * 1024,
            "guest RAM must be at least 128 MiB"
        );
        println!("Guest RAM: {} MiB", media.ram_size / (1024 * 1024));
        println!("SMP CPUs advertised: {smp_cpus}");
        let watchdog_ms = env_u64("BRIDGEVM_BOOT_PROBE_WATCHDOG_MS", WATCHDOG_MS);
        let watchdog_enabled = !env_flag("BRIDGEVM_BOOT_PROBE_WATCHDOG_DISABLED");
        if watchdog_enabled {
            println!("Boot watchdog: {watchdog_ms} ms per boot generation");
        } else {
            println!("Boot watchdog: disabled; guest/user shutdown required");
        }
        let trace_fwcfg = env_flag("BRIDGEVM_TRACE_FWCFG");
        let trace_msix = env_flag("BRIDGEVM_TRACE_MSIX");
        let trace_spi = env_flag("BRIDGEVM_TRACE_SPI");
        let trace_run_loop = env_flag("BRIDGEVM_TRACE_RUN_LOOP");
        let trace_xhci_bringup = env_flag("BRIDGEVM_TRACE_XHCI_BRINGUP");
        let smp_trace_enabled = env_flag("BRIDGEVM_SMP_TRACE");
        let stop_on_linux = env_flag_default("BRIDGEVM_BOOT_PROBE_STOP_ON_LINUX", true);
        Self {
            media,
            smp_cpus,
            swtpm_data_socket,
            swtpm_control_socket,
            platform_cfg,
            ram_size,
            watchdog_ms,
            watchdog_enabled,
            trace_fwcfg,
            trace_msix,
            trace_spi,
            trace_run_loop,
            trace_xhci_bringup,
            smp_trace_enabled,
            stop_on_linux,
        }
    }
}
