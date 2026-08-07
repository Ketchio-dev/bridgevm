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
        watchdog_ms: 600_000,
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
    // Nothing beyond the allowlist.
    assert_eq!(env.len(), 9);
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
        watchdog_ms: 1000,
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
