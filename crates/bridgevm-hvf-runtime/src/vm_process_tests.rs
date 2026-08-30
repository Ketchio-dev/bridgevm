//! The helper environment is exactly the manifest, nothing else.

use super::*;
use crate::manifest::LaunchManifest;
use std::io::Write;
use std::path::Path;

fn manifest_for(disk: &Path, vars: &Path) -> LaunchManifest {
    let text = format!(
        r#"{{"version": 1, "disk": "{}", "uefi_vars": "{}", "ram_mib": 6144, "vcpus": 4}}"#,
        disk.display(),
        vars.display()
    );
    LaunchManifest::parse(&text, false).expect("valid manifest")
}

fn scratch(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir();
    let disk = dir.join(format!("bv-vmproc-{tag}-{}.raw", std::process::id()));
    let vars = dir.join(format!("bv-vmproc-{tag}-{}.fd", std::process::id()));
    std::fs::write(&disk, b"disk").unwrap();
    std::fs::write(&vars, b"vars").unwrap();
    (disk, vars)
}

#[test]
fn the_environment_is_the_manifest_and_only_the_manifest() {
    let (disk, vars) = scratch("env");
    let manifest = manifest_for(&disk, &vars);
    let launch = HelperLaunch {
        helper: "/bin/true".into(),
        firmware_code: "/fw/code.fd".into(),
        watchdog_ms: Some(600_000),
        agent_control: None,
        surfaces: None,
        swtpm_sockets: None,
    };
    let env = helper_env(&manifest, &launch);
    // Every value traces to the manifest or the launch facts.
    let get = |k: &str| {
        env.iter()
            .find(|(name, _)| *name == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("{k} missing"))
    };
    assert_eq!(get("BRIDGEVM_NVME_DISK"), disk.display().to_string());
    assert_eq!(
        get("BRIDGEVM_AARCH64_UEFI_VARS"),
        vars.display().to_string()
    );
    assert_eq!(get("BRIDGEVM_AARCH64_UEFI_CODE"), "/fw/code.fd");
    assert_eq!(get("BRIDGEVM_RAM_MIB"), "6144");
    assert_eq!(get("BRIDGEVM_SMP_CPUS"), "4");
    assert_eq!(get("BRIDGEVM_BOOT_PROBE_WATCHDOG_MS"), "600000");
    // Product mode is not optional: reset means process recreation.
    assert_eq!(get("BRIDGEVM_EXIT_ON_RESET"), "1");
    // Nothing beyond the allowlist (headless boots disable xHCI).
    assert_eq!(env.len(), 10);
    let _ = std::fs::remove_file(&disk);
    let _ = std::fs::remove_file(&vars);
}

#[test]
fn spawn_gets_a_cleared_environment_and_the_generation() {
    let (disk, vars) = scratch("spawn");
    let manifest = manifest_for(&disk, &vars);
    // A shell script that prints its entire environment proves env_clear:
    // anything inherited from cargo/test env would show up here.
    let dir = std::env::temp_dir();
    let script = dir.join(format!("bv-vmproc-script-{}.sh", std::process::id()));
    let out_path = dir.join(format!("bv-vmproc-out-{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_file(&out_path);
    {
        let mut file = std::fs::File::create(&script).unwrap();
        writeln!(file, "#!/bin/sh\nenv > {}", out_path.display()).unwrap();
    }
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&script, perms).unwrap();

    let launch = HelperLaunch {
        helper: script.clone(),
        firmware_code: "/fw/code.fd".into(),
        watchdog_ms: Some(1000),
        agent_control: None,
        surfaces: None,
        swtpm_sockets: None,
    };
    let mut child = spawn_helper(&manifest, &launch, 7).expect("spawn");
    assert!(child.wait().expect("wait").success());
    let out = std::fs::read_to_string(&out_path).expect("env dump");
    assert!(out.contains("BRIDGEVM_RESET_GENERATION=7"));
    assert!(out.contains("BRIDGEVM_EXIT_ON_RESET=1"));
    // The test process's own environment must NOT leak through. CARGO_* is
    // guaranteed present in the parent under `cargo test`.
    assert!(
        !out.contains("CARGO"),
        "inherited environment leaked into the helper: {out}"
    );
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&disk);
    let _ = std::fs::remove_file(&vars);
}

#[test]
fn the_agent_console_surface_appears_only_when_asked_for() {
    let (disk, vars) = scratch("agent");
    let manifest = manifest_for(&disk, &vars);
    let launch = HelperLaunch {
        helper: "/bin/true".into(),
        firmware_code: "/fw/code.fd".into(),
        watchdog_ms: Some(700_000),
        agent_control: Some("/run/agent.ctl".into()),
        surfaces: None,
        swtpm_sockets: None,
    };
    let env = helper_env(&manifest, &launch);
    let get = |k: &str| {
        env.iter()
            .find(|(name, _)| *name == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("{k} missing"))
    };
    assert_eq!(get("BRIDGEVM_VIRTIO_CONSOLE_CTL"), "/run/agent.ctl");
    assert_eq!(get("BRIDGEVM_VIRTIO_CONSOLE_SERVICE"), "1");
    // The service timeout follows the watchdog: the agent outliving the
    // boot deadline would keep a dead boot looking alive.
    assert_eq!(get("BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS"), "700000");
    assert_eq!(env.len(), 17, "agent console + headless xhci-off");
    let _ = std::fs::remove_file(&disk);
    let _ = std::fs::remove_file(&vars);
}

#[test]
fn the_app_surfaces_reproduce_the_wrapper_device_shape() {
    let (disk, vars) = scratch("surfaces");
    let manifest = manifest_for(&disk, &vars);
    let launch = HelperLaunch {
        helper: "/bin/true".into(),
        firmware_code: "/fw/code.fd".into(),
        watchdog_ms: Some(600_000),
        agent_control: None,
        surfaces: Some(DeviceSurfaces {
            evidence_dir: "/ev".into(),
            display_export_ms: 100,
            input_control: Some("/ev/input.ctl".into()),
            virtio_gpu_3d: Some(GpuSurface {
                virgl: false,
                device_id: None,
            }),
            aggressive_performance: true,
            nvme_buffered_io: true,
            clipboard_sync: true,
            share: Some(ShareSurface {
                host_dir: "/host/share".into(),
                guest_dir: "C:\\BVShare".to_string(),
                interval_ms: 2000,
                max_kb: 65536,
            }),
            virtio_net: true,
            hda_audio: true,
        }),
        swtpm_sockets: None,
    };
    let env = helper_env(&manifest, &launch);
    let get = |k: &str| {
        env.iter()
            .find(|(name, _)| *name == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("{k} missing"))
    };
    assert_eq!(get("BRIDGEVM_RAMFB_DUMP_DIR"), "/ev/ramfb");
    assert_eq!(get("BRIDGEVM_DISPLAY_EXPORT_FB"), "/ev/display.fb");
    // PPM stays off: it checksums and rewrites a frame per interval, unread.
    assert!(!env.iter().any(|(k, _)| *k == "BRIDGEVM_DISPLAY_EXPORT_PPM"));
    assert_eq!(get("BRIDGEVM_DISPLAY_EXPORT_MS"), "100");
    // Input present means xHCI stays ON: no BRIDGEVM_DISABLE_XHCI.
    assert_eq!(get("BRIDGEVM_INPUT_CONTROL"), "/ev/input.ctl");
    assert!(!env.iter().any(|(k, _)| *k == "BRIDGEVM_DISABLE_XHCI"));
    // The venus set matches the wrapper's defaults byte for byte.
    assert_eq!(get("BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL"), "venus");
    assert_eq!(
        get("BRIDGEVM_VULKAN_LIB"),
        "/opt/homebrew/lib/libMoltenVK.dylib"
    );
    assert_eq!(get("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB"), "512");
    assert_eq!(
        get("BRIDGEVM_VIRTIO_GPU_TRACE_JSONL"),
        "/ev/virtio-gpu.jsonl"
    );
    // Clipboard, share, net, audio: the wrapper's exact contract.
    assert_eq!(get("BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC"), "1");
    assert_eq!(
        get("BRIDGEVM_VIRTIO_CONSOLE_SHARE"),
        "/host/share::C:\\BVShare"
    );
    assert_eq!(get("BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS"), "2000");
    assert_eq!(get("BRIDGEVM_VIRTIO_CONSOLE_SHARE_MAX_KB"), "65536");
    assert_eq!(get("BRIDGEVM_VIRTIO_NET_BACKEND"), "nat");
    assert_eq!(get("BRIDGEVM_HDA_COREAUDIO"), "1");
    assert_eq!(get("BRIDGEVM_NVME_BUFFERED_IO"), "1");
    // The aggressive lane uses IOSurface for the live display, while CPU
    // readback stays at the display-export cadence for evidence/fallback.
    // A zero interval means uncapped readback, not disabled readback.
    assert_eq!(get("BRIDGEVM_VIRTIO_GPU_DIRECT_RENDERER"), "1");
    assert_eq!(get("BRIDGEVM_VIRTIO_GPU_ASYNC_SCANOUT"), "1");
    assert_eq!(get("BRIDGEVM_VIRTIO_GPU_IOSURFACE_SCANOUT"), "1");
    let readbacks: Vec<&str> = env
        .iter()
        .filter(|(k, _)| *k == "BRIDGEVM_VIRTIO_GPU_SCANOUT_READBACK_MS")
        .map(|(_, v)| v.as_str())
        .collect();
    assert_eq!(readbacks, vec!["100"]);
    let _ = std::fs::remove_file(&disk);
    let _ = std::fs::remove_file(&vars);
}

#[test]
fn surfaced_spawns_append_to_the_run_log_across_generations() {
    let (disk, vars) = scratch("runlog");
    let manifest = manifest_for(&disk, &vars);
    let dir = std::env::temp_dir().join(format!("bv-vmproc-ev-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let launch = HelperLaunch {
        helper: "/bin/echo".into(),
        firmware_code: "/fw/code.fd".into(),
        watchdog_ms: Some(1000),
        agent_control: None,
        surfaces: Some(DeviceSurfaces {
            evidence_dir: dir.clone(),
            display_export_ms: 100,
            input_control: None,
            virtio_gpu_3d: None,
            aggressive_performance: false,
            nvme_buffered_io: false,
            clipboard_sync: false,
            share: None,
            virtio_net: false,
            hda_audio: false,
        }),
        swtpm_sockets: None,
    };
    // Each generation echoes its number; both lines must survive in order
    // (append, not truncate) because the app tails this file across resets.
    for generation in 0..2 {
        let mut child = spawn_helper(&manifest, &launch, generation).unwrap();
        assert!(child.wait().unwrap().success());
    }
    let log = std::fs::read_to_string(dir.join("run.log")).expect("the app's tail target");
    assert_eq!(log, "\n\n", "two echo generations, appended not truncated");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&disk);
    let _ = std::fs::remove_file(&vars);
}

#[test]
fn no_watchdog_is_the_app_mode_and_disables_rather_than_zeroes() {
    let (disk, vars) = scratch("nowd");
    let manifest = manifest_for(&disk, &vars);
    let launch = HelperLaunch {
        helper: "/bin/true".into(),
        firmware_code: "/fw/code.fd".into(),
        watchdog_ms: None,
        agent_control: Some("/run/agent.ctl".into()),
        surfaces: None,
        swtpm_sockets: None,
    };
    let env = helper_env(&manifest, &launch);
    let get = |k: &str| {
        env.iter()
            .find(|(name, _)| *name == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("{k} missing"))
    };
    assert_eq!(get("BRIDGEVM_BOOT_PROBE_WATCHDOG_DISABLED"), "1");
    assert!(
        !env.iter()
            .any(|(k, _)| *k == "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS"),
        "disabled means absent, not zero -- zero would be a 0ms watchdog"
    );
    // The agent service still gets a real budget (the wrapper's day).
    assert_eq!(get("BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS"), "86400000");
    let _ = std::fs::remove_file(&disk);
    let _ = std::fs::remove_file(&vars);
}

#[test]
fn the_app_ships_virgl_1050_and_the_env_says_so() {
    let (disk, vars) = scratch("virgl");
    let manifest = manifest_for(&disk, &vars);
    let launch = HelperLaunch {
        helper: "/bin/true".into(),
        firmware_code: "/fw/code.fd".into(),
        watchdog_ms: None,
        agent_control: None,
        surfaces: Some(DeviceSurfaces {
            evidence_dir: "/ev".into(),
            display_export_ms: 100,
            input_control: None,
            virtio_gpu_3d: Some(GpuSurface {
                virgl: true,
                device_id: Some("1050".to_string()),
            }),
            aggressive_performance: true,
            nvme_buffered_io: true,
            clipboard_sync: false,
            share: None,
            virtio_net: false,
            hda_audio: false,
        }),
        swtpm_sockets: None,
    };
    let env = helper_env(&manifest, &launch);
    let get = |k: &str| {
        env.iter()
            .find(|(name, _)| *name == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("{k} missing"))
    };
    assert_eq!(get("BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL"), "virgl");
    assert_eq!(get("BRIDGEVM_VIRTIO_GPU_PCI_DEVICE_ID"), "0x1050");
    // virgl must NOT get the venus-only extras: MoltenVK in-process and the
    // oversized BAR2 exist for Vulkan passthrough, not the GL path.
    assert!(!env.iter().any(|(k, _)| *k == "BRIDGEVM_VULKAN_LIB"));
    assert!(!env
        .iter()
        .any(|(k, _)| *k == "BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB"));
    assert!(!env
        .iter()
        .any(|(k, _)| *k == "BRIDGEVM_VIRTIO_GPU_3D_BIND_ID"));
    let _ = std::fs::remove_file(&disk);
    let _ = std::fs::remove_file(&vars);
}
