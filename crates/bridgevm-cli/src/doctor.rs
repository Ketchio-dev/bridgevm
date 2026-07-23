//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn doctor_audit_for_paths(store_root: &Path, vms_dir: &Path) -> Vec<DoctorCheck> {
    let path_dirs = env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect())
        .unwrap_or_default();
    doctor_audit(&DoctorAuditInput {
        store_root: store_root.to_path_buf(),
        vms_dir: vms_dir.to_path_buf(),
        path_dirs,
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
    })
}

pub(crate) fn doctor_audit(input: &DoctorAuditInput) -> Vec<DoctorCheck> {
    let qemu_img = find_executable("qemu-img", &input.path_dirs);
    let qemu_aarch64 = find_executable("qemu-system-aarch64", &input.path_dirs);
    let qemu_x86_64 = find_executable("qemu-system-x86_64", &input.path_dirs);
    let lightvm_runner = find_executable("lightvm-runner", &input.path_dirs);
    let fullvm_runner = find_executable("fullvm-runner", &input.path_dirs);
    let networkd = find_executable("networkd", &input.path_dirs);
    let is_macos = input.os == "macos";
    let is_apple_silicon = matches!(input.arch.as_str(), "aarch64" | "arm64");

    let mut checks = Vec::new();
    checks.push(path_dir_check("Store root", &input.store_root));
    checks.push(path_dir_check("VM bundles dir", &input.vms_dir));
    checks.push(executable_check(
        "qemu-img",
        qemu_img.as_deref(),
        "required for qcow2 disk creation and snapshot overlays",
    ));

    match (qemu_aarch64.as_deref(), qemu_x86_64.as_deref()) {
        (Some(aarch64), Some(x86_64)) => checks.push(DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "QEMU system binary".to_string(),
            detail: format!(
                "found qemu-system-aarch64 at {} and qemu-system-x86_64 at {}",
                aarch64.display(),
                x86_64.display()
            ),
        }),
        (Some(path), None) => checks.push(DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "QEMU system binary".to_string(),
            detail: format!("found qemu-system-aarch64 at {}", path.display()),
        }),
        (None, Some(path)) => checks.push(DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "QEMU system binary".to_string(),
            detail: format!("found qemu-system-x86_64 at {}", path.display()),
        }),
        (None, None) => checks.push(DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: "QEMU system binary".to_string(),
            detail: "qemu-system-aarch64 or qemu-system-x86_64 was not found on PATH".to_string(),
        }),
    }

    checks.push(optional_executable_check(
        "lightvm-runner",
        lightvm_runner.as_deref(),
        "Fast Mode runner candidate was not found on PATH",
    ));
    checks.push(optional_executable_check(
        "fullvm-runner",
        fullvm_runner.as_deref(),
        "Compatibility Mode runner candidate was not found on PATH",
    ));
    checks.push(optional_executable_check(
        "networkd",
        networkd.as_deref(),
        "network helper candidate was not found on PATH",
    ));
    checks.push(DoctorCheck {
        status: if is_macos {
            DoctorCheckStatus::Ok
        } else {
            DoctorCheckStatus::Warn
        },
        name: "macOS host".to_string(),
        detail: if is_macos {
            "current host reports macos".to_string()
        } else {
            format!(
                "current host reports {}; Apple Virtualization is macOS-only",
                input.os
            )
        },
    });
    checks.push(DoctorCheck {
        status: if is_apple_silicon {
            DoctorCheckStatus::Ok
        } else {
            DoctorCheckStatus::Warn
        },
        name: "Apple Silicon host".to_string(),
        detail: if is_apple_silicon {
            format!("current host arch is {}", input.arch)
        } else {
            format!(
                "current host arch is {}; arm64/aarch64 is expected for Fast Mode",
                input.arch
            )
        },
    });
    checks.push(DoctorCheck {
        status: if is_macos && is_apple_silicon && lightvm_runner.is_some() {
            DoctorCheckStatus::Ok
        } else if is_macos && is_apple_silicon {
            DoctorCheckStatus::Warn
        } else {
            DoctorCheckStatus::Missing
        },
        name: "Fast Mode possibility".to_string(),
        detail: fast_mode_detail(is_macos, is_apple_silicon, lightvm_runner.as_deref()),
    });

    checks
}

pub(crate) fn path_dir_check(name: &str, path: &Path) -> DoctorCheck {
    if path.is_dir() {
        DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: name.to_string(),
            detail: format!("{} exists", path.display()),
        }
    } else if path.exists() {
        DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: name.to_string(),
            detail: format!("{} exists but is not a directory", path.display()),
        }
    } else {
        DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: name.to_string(),
            detail: format!("{} does not exist", path.display()),
        }
    }
}

pub(crate) fn executable_check(
    name: &str,
    path: Option<&Path>,
    missing_detail: &str,
) -> DoctorCheck {
    match path {
        Some(path) => DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: name.to_string(),
            detail: format!("found at {}", path.display()),
        },
        None => DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: name.to_string(),
            detail: missing_detail.to_string(),
        },
    }
}

pub(crate) fn optional_executable_check(
    name: &str,
    path: Option<&Path>,
    missing_detail: &str,
) -> DoctorCheck {
    match path {
        Some(path) => DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: name.to_string(),
            detail: format!("found at {}", path.display()),
        },
        None => DoctorCheck {
            status: DoctorCheckStatus::Warn,
            name: name.to_string(),
            detail: missing_detail.to_string(),
        },
    }
}

pub(crate) fn fast_mode_detail(
    is_macos: bool,
    is_apple_silicon: bool,
    runner: Option<&Path>,
) -> String {
    match (is_macos, is_apple_silicon, runner) {
        (true, true, Some(path)) => {
            format!(
                "macOS Apple Silicon host with lightvm-runner at {}",
                path.display()
            )
        }
        (true, true, None) => {
            "macOS Apple Silicon host detected, but lightvm-runner is not on PATH".to_string()
        }
        (false, _, _) => "Fast Mode requires macOS with Apple Virtualization".to_string(),
        (true, false, _) => "Fast Mode requires an Apple Silicon host".to_string(),
    }
}

pub(crate) fn find_executable(name: &str, path_dirs: &[PathBuf]) -> Option<PathBuf> {
    path_dirs
        .iter()
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable_file(candidate))
}

pub(crate) fn is_executable_file(path: &Path) -> bool {
    path.is_file()
        && path
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

pub(crate) fn print_doctor_audit(checks: &[DoctorCheck]) {
    println!("Host capability audit:");
    for check in checks {
        println!(
            "[{}] {}: {}",
            check.status.as_str(),
            check.name,
            check.detail
        );
    }
}

pub(crate) fn print_engine_catalog(descriptors: &[VmEngineDescriptor]) {
    println!("Engine lanes:");
    for descriptor in descriptors {
        println!(
            "[{}] {} ({}): {}",
            descriptor.product_state.as_str(),
            descriptor.label,
            descriptor.lane.id(),
            descriptor.product_state_detail
        );
        println!("    Substrate: {}", descriptor.substrate);
        println!("    Guest scope: {}", descriptor.guest_scope);
        println!(
            "    Windows 11 Arm role: {}",
            descriptor.windows_11_arm_role
        );
        println!("    QEMU: {}", descriptor.qemu_usage);
    }
}

pub(crate) fn parallels_class_progress() -> Vec<ProductTrackProgress> {
    vec![
        ProductTrackProgress {
            status: ProductTrackStatus::Partial,
            name: "macOS-native integration / Coherence",
            implemented:
                "clipboard/display resize foundations plus preserved Linux .desktop/gio/gtk-launch/wmctrl live GUI proof and crop/proxy plumbing",
            next: "drive real guest-window crops from real framebuffer/proxy sessions, then move toward compositor-grade host-window integration",
        },
        ProductTrackProgress {
            status: ProductTrackStatus::Proven,
            name: "Apple Silicon Fast Mode",
            implemented:
                "Apple Virtualization.framework path with live Linux Arm64 boot/suspend/resume and VZVirtualMachineView display",
            next: "broaden boot shapes and keep app/daemon/helper IPC tight",
        },
        ProductTrackProgress {
            status: ProductTrackStatus::Partial,
            name: "intelligent resources / battery",
            implemented:
                "power-aware launch policy, display pacing consumption, and runtime policy IPC",
            next: "live Apple VZ CPU/RAM control must apply the policy to a running VM",
        },
        ProductTrackProgress {
            status: ProductTrackStatus::Planned,
            name: "graphics acceleration / Metal",
            implemented: "native VZ GUI pixels are proven in an AppKit display window",
            next: "Metal compositor/frame pacing first; Direct3D-to-Metal or WDDM remains long-term R&D",
        },
    ]
}

pub(crate) fn print_parallels_class_progress(progress: &[ProductTrackProgress]) {
    println!("Parallels-class progress:");
    for track in progress {
        println!(
            "[{}] {}: {}",
            track.status.as_str(),
            track.name,
            track.implemented
        );
        println!("    Next: {}", track.next);
    }
}

impl From<SnapshotKindChoice> for SnapshotKind {
    fn from(value: SnapshotKindChoice) -> Self {
        match value {
            SnapshotKindChoice::Disk => SnapshotKind::Disk,
            SnapshotKindChoice::Suspend => SnapshotKind::Suspend,
            SnapshotKindChoice::ApplicationConsistent => SnapshotKind::ApplicationConsistent,
        }
    }
}
