//! Spawning the VM helper process from a validated manifest -- no shell.
//!
//! The wrapper script builds the helper's environment from CLI flags and
//! scrubs inherited `BRIDGEVM_*` as a policy boundary. The product runtime
//! goes further: `env_clear()` plus an explicit allowlist derived only from
//! the parsed manifest and the host launch facts, so the helper's entire
//! configuration is this function's return value. Product mode always sets
//! `BRIDGEVM_EXIT_ON_RESET=1` -- in-process reboot is diagnostic-only
//! (PLAN.md R1); a guest SYSTEM_RESET must end the process for recreation.

use crate::manifest::LaunchManifest;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

/// Host facts a launch needs beyond the manifest: which helper binary,
/// which firmware code image, how long a boot may stall, and (optionally)
/// the agent console control file through which the host drives the guest.
pub struct HelperLaunch {
    pub helper: PathBuf,
    pub firmware_code: PathBuf,
    /// Per-boot diagnostic watchdog. None is the app's normal mode: the VM
    /// stays up until the guest or the user asks otherwise.
    pub watchdog_ms: Option<u64>,
    /// When set, the helper runs the resident agent console service and
    /// reads guest commands appended to this file. This is how a supervisor
    /// asks the guest for a reset without any shell in between.
    pub agent_control: Option<PathBuf>,
    /// The app-facing device surfaces; None boots headless (soak mode).
    pub surfaces: Option<DeviceSurfaces>,
    /// swtpm sockets when the supervisor runs a vTPM (crate::start_swtpm);
    /// the probe attaches its TPM2 TIS device iff the data socket is set.
    pub swtpm_sockets: Option<(PathBuf, PathBuf)>,
}

/// What the product app wires beyond bare boot: display out, input in,
/// and (when the guest image carries the driver) the venus 3D GPU.
///
/// These mirror the wrapper script's flags exactly -- same env contract,
/// same defaults -- so a boot through the typed path presents the same
/// device shape the evidence gates validated.
pub struct DeviceSurfaces {
    /// Directory for ramfb dumps, display exports, and the GPU trace.
    /// The helper's stdout/stderr also land here as `run.log`, appended
    /// across generations, which is the transcript the app tails.
    pub evidence_dir: PathBuf,
    /// Milliseconds between display exports (the app's screen poll rate).
    pub display_export_ms: u64,
    /// xHCI keyboard/pointer plus the live-input control file.
    pub input_control: Option<PathBuf>,
    /// virtio-gpu 3D. The protocol and PCI identity follow the app's
    /// shipping choice; venus additionally wires MoltenVK + BAR2 sizing.
    pub virtio_gpu_3d: Option<GpuSurface>,
    /// The app's shipping renderer lane (wrapper --performance-risk
    /// aggressive): direct renderer, async scanout, and IOSurface scanout.
    /// CPU readback remains paced at the display-export cadence for evidence
    /// and fallback consumers. Launch policy only -- media is untouched.
    pub aggressive_performance: bool,
    /// Buffered NVMe I/O (the app ships it on).
    pub nvme_buffered_io: bool,
    /// Host<->guest clipboard sync over the agent console.
    pub clipboard_sync: bool,
    /// Folder share over the agent console (host dir -> guest dir).
    pub share: Option<ShareSurface>,
    /// virtio-net with the NAT backend.
    pub virtio_net: bool,
    /// Intel HDA audio played through CoreAudio.
    pub hda_audio: bool,
}

/// The 3D GPU's protocol and PCI identity.
pub struct GpuSurface {
    /// virgl (the app's shipping protocol, device 1050) or venus.
    pub virgl: bool,
    /// Explicit PCI device id (hex, no 0x -- "1050"/"10F7"); None binds the
    /// probe's default 3D identity.
    pub device_id: Option<String>,
}

/// One shared folder, with the app's shipping cadence and size cap.
pub struct ShareSurface {
    pub host_dir: PathBuf,
    pub guest_dir: String,
    pub interval_ms: u64,
    pub max_kb: u64,
}

/// The helper's complete environment. Nothing else reaches the child.
pub fn helper_env(manifest: &LaunchManifest, launch: &HelperLaunch) -> Vec<(&'static str, String)> {
    let mut env = vec![
        ("BRIDGEVM_NVME_DISK", manifest.disk().to_string()),
        ("BRIDGEVM_NVME_DISK_WRITABLE", "1".to_string()),
        (
            "BRIDGEVM_AARCH64_UEFI_VARS",
            manifest.uefi_vars().to_string(),
        ),
        ("BRIDGEVM_AARCH64_UEFI_VARS_WRITABLE", "1".to_string()),
        (
            "BRIDGEVM_AARCH64_UEFI_CODE",
            launch.firmware_code.display().to_string(),
        ),
        ("BRIDGEVM_RAM_MIB", manifest.ram_mib().to_string()),
        ("BRIDGEVM_SMP_CPUS", manifest.vcpus().to_string()),
        ("BRIDGEVM_EXIT_ON_RESET", "1".to_string()),
    ];
    match launch.watchdog_ms {
        Some(watchdog_ms) => env.push(("BRIDGEVM_BOOT_PROBE_WATCHDOG_MS", watchdog_ms.to_string())),
        None => env.push(("BRIDGEVM_BOOT_PROBE_WATCHDOG_DISABLED", "1".to_string())),
    }
    if let Some(surfaces) = &launch.surfaces {
        let evidence = |name: &str| surfaces.evidence_dir.join(name).display().to_string();
        env.push(("BRIDGEVM_RAMFB", "1".to_string()));
        env.push(("BRIDGEVM_RAMFB_DUMP_DIR", evidence("ramfb")));
        // No PPM feed: the app reads display.fb, and the PPM path costs a full
        // frame checksum and file rewrite per interval for a file nobody opens.
        env.push(("BRIDGEVM_DISPLAY_EXPORT_FB", evidence("display.fb")));
        env.push((
            "BRIDGEVM_DISPLAY_EXPORT_MS",
            surfaces.display_export_ms.to_string(),
        ));
        env.push((
            "BRIDGEVM_VIRTIO_GPU_SCANOUT_READBACK_MS",
            surfaces.display_export_ms.to_string(),
        ));
        match &surfaces.input_control {
            Some(control) => {
                env.push(("BRIDGEVM_INPUT_CONTROL", control.display().to_string()));
            }
            None => {
                env.push(("BRIDGEVM_DISABLE_XHCI", "1".to_string()));
            }
        }
        if surfaces.clipboard_sync {
            env.push(("BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC", "1".to_string()));
        }
        if let Some(share) = &surfaces.share {
            env.push((
                "BRIDGEVM_VIRTIO_CONSOLE_SHARE",
                format!("{}::{}", share.host_dir.display(), share.guest_dir),
            ));
            env.push((
                "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS",
                share.interval_ms.to_string(),
            ));
            env.push((
                "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MAX_KB",
                share.max_kb.to_string(),
            ));
        }
        if surfaces.virtio_net {
            env.push(("BRIDGEVM_VIRTIO_NET", "1".to_string()));
            env.push(("BRIDGEVM_VIRTIO_NET_BACKEND", "nat".to_string()));
        }
        if surfaces.hda_audio {
            env.push(("BRIDGEVM_HDA", "1".to_string()));
            env.push(("BRIDGEVM_HDA_COREAUDIO", "1".to_string()));
        }
        if surfaces.nvme_buffered_io {
            env.push(("BRIDGEVM_NVME_BUFFERED_IO", "1".to_string()));
        }
        if let Some(gpu) = &surfaces.virtio_gpu_3d {
            env.push(("BRIDGEVM_VIRTIO_GPU", "1".to_string()));
            env.push(("BRIDGEVM_VIRTIO_GPU_3D", "1".to_string()));
            env.push((
                "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL",
                if gpu.virgl { "virgl" } else { "venus" }.to_string(),
            ));
            if surfaces.aggressive_performance {
                env.push(("BRIDGEVM_VIRTIO_GPU_DIRECT_RENDERER", "1".to_string()));
                env.push(("BRIDGEVM_VIRTIO_GPU_ASYNC_SCANOUT", "1".to_string()));
                env.push(("BRIDGEVM_VIRTIO_GPU_IOSURFACE_SCANOUT", "1".to_string()));
            }
            if !gpu.virgl {
                // venus needs MoltenVK in process and a BAR2 EDK2 can assign.
                env.push((
                    "BRIDGEVM_VULKAN_LIB",
                    "/opt/homebrew/lib/libMoltenVK.dylib".to_string(),
                ));
                env.push(("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB", "512".to_string()));
            }
            match &gpu.device_id {
                Some(device_id) => env.push((
                    "BRIDGEVM_VIRTIO_GPU_PCI_DEVICE_ID",
                    format!("0x{device_id}"),
                )),
                None => env.push(("BRIDGEVM_VIRTIO_GPU_3D_BIND_ID", "1".to_string())),
            }
            env.push((
                "BRIDGEVM_VIRTIO_GPU_TRACE_JSONL",
                evidence("virtio-gpu.jsonl"),
            ));
        }
    } else {
        env.push(("BRIDGEVM_DISABLE_XHCI", "1".to_string()));
    }
    if let Some((data, control)) = &launch.swtpm_sockets {
        env.push(("BRIDGEVM_SWTPM_DATA_SOCKET", data.display().to_string()));
        env.push((
            "BRIDGEVM_SWTPM_CONTROL_SOCKET",
            control.display().to_string(),
        ));
    }
    if let Some(control) = &launch.agent_control {
        env.push(("BRIDGEVM_VIRTIO_CONSOLE", "1".to_string()));
        env.push(("BRIDGEVM_VIRTIO_CONSOLE_TEST", "1".to_string()));
        env.push(("BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC", "1".to_string()));
        env.push(("BRIDGEVM_VIRTIO_CONSOLE_CMDS", "whoami".to_string()));
        env.push((
            "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS",
            // The agent service follows the watchdog; unwatched runs get
            // the wrapper's no-watchdog service budget of a day.
            launch.watchdog_ms.unwrap_or(86_400_000).to_string(),
        ));
        env.push(("BRIDGEVM_VIRTIO_CONSOLE_SERVICE", "1".to_string()));
        env.push(("BRIDGEVM_VIRTIO_CONSOLE_CTL", control.display().to_string()));
    }
    env
}

/// Spawn one helper generation: argv only, cleared environment. With
/// surfaces, stdout/stderr append to `<evidence_dir>/run.log` -- the file
/// the app tails -- so the transcript survives generations; headless runs
/// inherit stdio so boot evidence lands in the supervisor's own transcript.
pub fn spawn_helper(
    manifest: &LaunchManifest,
    launch: &HelperLaunch,
    generation: u64,
) -> std::io::Result<Child> {
    let mut command = Command::new(&launch.helper);
    command
        .env_clear()
        .envs(
            helper_env(manifest, launch)
                .into_iter()
                .map(|(k, v)| (k.to_string(), v)),
        )
        .env("BRIDGEVM_RESET_GENERATION", generation.to_string())
        .stdin(Stdio::null());
    if let Some(surfaces) = &launch.surfaces {
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(surfaces.evidence_dir.join("run.log"))?;
        let log_err = log.try_clone()?;
        command.stdout(log).stderr(log_err);
    }
    command.spawn()
}

#[cfg(test)]
#[path = "vm_process_tests.rs"]
mod tests;
