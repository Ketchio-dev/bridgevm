//! Store results and client checks have distinct scopes and summaries.

use crate::*;

pub(crate) fn doctor(store: &VmStore) -> Result<()> {
    store.ensure().context("failed to prepare BridgeVM store")?;
    print_doctor_report(
        store.root(),
        &store.vms_dir(),
        "OK",
        "local",
        &doctor_audit_for_current_host(store),
    );
    Ok(())
}

pub(crate) fn print_daemon_doctor(store_root: &Path, vms_dir: &Path, status: &str) {
    print_doctor_report(
        store_root,
        vms_dir,
        status,
        "daemon-reported",
        &doctor_audit_for_paths(store_root, vms_dir),
    );
}

fn print_doctor_report(
    store_root: &Path,
    vms_dir: &Path,
    store_status: &str,
    store_source: &str,
    checks: &[DoctorCheck],
) {
    println!("BridgeVM store ({store_source}): {}", store_root.display());
    println!("VM bundles ({store_source}): {}", vms_dir.display());
    println!("Store status ({store_source}): {store_status}");
    print_doctor_audit(checks);
    println!("{}", environment_summary(checks));
    println!("QEMU checks cover compatibility disk/run tools; own-HVF runtime readiness is not assessed.");
    print_engine_catalog(available_engine_descriptors());
    print_parallels_class_progress(&parallels_class_progress());
}

fn print_doctor_audit(checks: &[DoctorCheck]) {
    println!("Client environment checks (paths and tools on this client):");
    for check in checks {
        println!(
            "[{}] {}: {}",
            check.status.as_str(),
            check.name,
            check.detail
        );
    }
}

fn environment_summary(checks: &[DoctorCheck]) -> String {
    let (mut ok, mut warn, mut missing) = (0, 0, 0);
    for check in checks {
        match check.status {
            DoctorCheckStatus::Ok => ok += 1,
            DoctorCheckStatus::Warn => warn += 1,
            DoctorCheckStatus::Missing => missing += 1,
        }
    }
    let status = if missing > 0 {
        "MISSING"
    } else if warn > 0 {
        "WARN"
    } else if ok > 0 {
        "OK"
    } else {
        "NOT CHECKED"
    };
    format!("Client environment summary: {status} (OK: {ok}, WARN: {warn}, MISSING: {missing})")
}

#[cfg(test)]
#[path = "doctor_summary_tests.rs"]
mod tests;
