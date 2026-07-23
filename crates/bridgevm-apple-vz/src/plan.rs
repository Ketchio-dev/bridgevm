//! AppleVzPlan, AppleVzLinuxConfig, and the manifest-to-plan build.

use crate::*;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use bridgevm_resource_manager::decide_from_manifest_profile;
use bridgevm_resource_manager::resolve_memory;
use bridgevm_resource_manager::resolve_vcpu;
use serde::Deserialize;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzLinuxConfig {
    pub kernel_path: Option<String>,
    pub disk_path: String,
    pub memory: String,
    pub cpu: String,
    pub virtiofs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzPlan {
    pub vm_name: String,
    pub guest_os: String,
    pub guest_arch: String,
    pub config: AppleVzLinuxConfig,
    pub launch_spec: AppleVzLaunchSpec,
    pub entropy_device: bool,
    pub balloon_device: bool,
    pub clipboard: bool,
    pub shared_folders: bool,
}

pub fn build_fast_plan(
    manifest: &VmManifest,
    bundle_path: &Path,
) -> Result<AppleVzPlan, AppleVzError> {
    if manifest.mode != VmMode::Fast {
        return Err(AppleVzError::UnsupportedMode(manifest.mode));
    }
    preflight_apple_vz_launch(manifest)?;

    let disk_path = resolve_bundle_path(bundle_path, &manifest.storage.primary.path);
    let disk_path = disk_path.display().to_string();
    let bundle_path = bundle_path.display().to_string();
    let runner_log_path = resolve_bundle_path(Path::new(&bundle_path), "logs/lightvm.log")
        .display()
        .to_string();
    let serial_log_path = resolve_bundle_path(Path::new(&bundle_path), "logs/serial.log")
        .display()
        .to_string();
    let balloon_device = manifest.resources.profile == "automatic";
    let resource_decision = decide_from_manifest_profile(&manifest.resources.profile);
    let memory = resolve_memory(&manifest.resources.memory, &resource_decision);
    let cpu = resolve_vcpu(&manifest.resources.cpu, &resource_decision);
    let boot = build_boot_spec(manifest, Path::new(&bundle_path))?;
    let readiness = build_readiness_spec(
        &boot,
        &disk_path,
        &manifest.storage.primary.format,
        &AppleVzHostCapability::current(),
    );
    let shares = build_share_specs(
        manifest.integration.shared_folders,
        &manifest.shared_folders,
    );
    let launch_spec = AppleVzLaunchSpec {
        vm_name: manifest.name.clone(),
        bundle_path: bundle_path.clone(),
        guest: AppleVzGuestSpec {
            os: manifest.guest.os.clone(),
            arch: manifest.guest.arch.clone(),
        },
        boot,
        disk: AppleVzDiskSpec {
            path: disk_path.clone(),
            format: manifest.storage.primary.format.clone(),
            read_only: false,
        },
        resources: AppleVzResourceSpec {
            memory: memory.clone(),
            cpu: cpu.clone(),
            display_fps_cap: resource_decision.display_fps_cap.clone(),
            rationale: resource_decision.rationale.clone(),
            balloon_device,
        },
        devices: AppleVzDeviceSpec {
            entropy_device: true,
            network: manifest.network.mode.clone(),
            serial_log_path,
        },
        integration: AppleVzIntegrationSpec {
            clipboard: manifest.integration.clipboard,
            dynamic_resolution: manifest.integration.dynamic_resolution,
            shared_folders: manifest.integration.shared_folders,
            virtiofs: manifest.integration.shared_folders,
        },
        logs: AppleVzLogSpec { runner_log_path },
        shares,
        readiness,
    };

    Ok(AppleVzPlan {
        vm_name: manifest.name.clone(),
        guest_os: manifest.guest.os.clone(),
        guest_arch: manifest.guest.arch.clone(),
        config: AppleVzLinuxConfig {
            kernel_path: launch_spec
                .boot
                .kernel
                .as_ref()
                .map(|kernel| kernel.path.clone()),
            disk_path: disk_path.clone(),
            memory,
            cpu,
            virtiofs: manifest.integration.shared_folders,
        },
        launch_spec,
        entropy_device: true,
        balloon_device,
        clipboard: manifest.integration.clipboard,
        shared_folders: manifest.integration.shared_folders,
    })
}

impl AppleVzLinuxConfig {
    pub fn automatic(disk_path: impl Into<String>) -> Self {
        Self {
            kernel_path: None,
            disk_path: disk_path.into(),
            memory: "auto".to_string(),
            cpu: "auto".to_string(),
            virtiofs: true,
        }
    }
}

impl AppleVzPlan {
    pub fn render_runner_words(&self) -> Vec<String> {
        vec!["lightvm-runner".to_string(), self.vm_name.clone()]
    }

    pub fn render_runner_words_for_launch_spec(&self, launch_spec_path: &Path) -> Vec<String> {
        vec![
            "lightvm-runner".to_string(),
            "--launch-spec".to_string(),
            launch_spec_path.display().to_string(),
        ]
    }

    pub fn launch_spec(&self) -> &AppleVzLaunchSpec {
        &self.launch_spec
    }
}
