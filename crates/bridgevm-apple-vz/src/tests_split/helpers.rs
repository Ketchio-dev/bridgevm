//! Split test module.

use crate::*;
use bridgevm_config::Guest;
use bridgevm_config::SharedFolder;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use std::path::Path;

pub(super) fn shared_folder(name: &str, host_path: &str, read_only: bool) -> SharedFolder {
    SharedFolder {
        name: name.to_string(),
        host_path: host_path.to_string(),
        read_only,
        host_path_token: None,
    }
}

pub(super) fn valid_fast_manifest() -> VmManifest {
    VmManifest::new(
        "Ubuntu Fast",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    )
}

#[test]
fn resource_profile_applies_to_auto_values_and_preserves_manual_overrides() {
    let mut manifest = valid_fast_manifest();
    manifest.resources.profile = "performance".to_string();
    let plan = build_fast_plan(&manifest, Path::new("/tmp/perf.vmbridge")).unwrap();
    assert_eq!(plan.launch_spec.resources.memory, "6144");
    assert_eq!(plan.launch_spec.resources.cpu, "4");
    assert_eq!(plan.launch_spec.resources.display_fps_cap, "60");
    assert_eq!(plan.config.memory, "6144");
    assert_eq!(plan.config.cpu, "4");

    manifest.resources.memory = "8192".to_string();
    manifest.resources.cpu = "6".to_string();
    let plan = build_fast_plan(&manifest, Path::new("/tmp/manual.vmbridge")).unwrap();
    assert_eq!(plan.launch_spec.resources.memory, "8192");
    assert_eq!(plan.launch_spec.resources.cpu, "6");
    assert_eq!(plan.config.memory, "8192");
    assert_eq!(plan.config.cpu, "6");
}

#[cfg(unix)]
#[test]
fn command_launcher_reports_helper_failure() {
    use std::os::unix::fs::PermissionsExt;

    let temp = std::env::temp_dir().join(format!(
        "bridgevm-apple-vz-command-launcher-fail-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let helper = temp.join("helper.sh");
    std::fs::write(
        &helper,
        "#!/bin/sh\ncat >/dev/null\necho 'ready summary on stdout'\necho 'not implemented yet' >&2\nexit 2\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&helper, permissions).unwrap();

    let manifest = valid_fast_manifest();
    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    let mut handoff = build_launch_handoff(plan.launch_spec(), None);
    handoff.readiness = AppleVzReadinessSpec {
        ready: true,
        blockers: Vec::new(),
    };

    let error = launch_with_apple_vz(&AppleVzCommandLauncher::new(&helper), handoff)
        .expect_err("helper failure must surface");

    match error {
        AppleVzLaunchError::LauncherFailed { stdout, stderr, .. } => {
            assert_eq!(stderr, "not implemented yet");
            assert_eq!(stdout, "ready summary on stdout");
        }
        other => panic!("expected helper failure, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn render_runner_words_emits_store_reload_command_without_stale_planner_flags() {
    let manifest = valid_fast_manifest();
    // Default manifest has no approved folders, so no shares are planned.
    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    let words = plan.render_runner_words();

    assert_eq!(
        words,
        vec!["lightvm-runner".to_string(), "Ubuntu Fast".to_string()]
    );
    assert!(plan.launch_spec.shares.is_empty());
    assert!(!words.iter().any(|w| w == "--apple-vz"));
    assert!(!words.iter().any(|w| w == "--disk"));
    assert!(!words.iter().any(|w| w == "--memory"));
    assert!(!words.iter().any(|w| w == "--cpu"));
    assert!(!words.iter().any(|w| w == "--share"));
    assert!(!words.iter().any(|w| w == "--share-dir"));
}

#[test]
fn build_launch_handoff_carries_all_shares_through() {
    let mut manifest = valid_fast_manifest();
    manifest.integration.shared_folders = true;
    manifest.shared_folders = vec![
        shared_folder("workspace", "/Users/me/work", true),
        shared_folder("docs", "/Users/me/docs", false),
    ];

    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    let handoff = build_launch_handoff(plan.launch_spec(), None);

    assert_eq!(handoff.shares.len(), 2);
    assert_eq!(handoff.shares[0].tag, "workspace");
    assert_eq!(handoff.shares[0].host_path, "/Users/me/work");
    assert!(handoff.shares[0].read_only);
    assert_eq!(handoff.shares[1].tag, "docs");
    assert_eq!(handoff.shares[1].host_path, "/Users/me/docs");
    assert!(!handoff.shares[1].read_only);
}
