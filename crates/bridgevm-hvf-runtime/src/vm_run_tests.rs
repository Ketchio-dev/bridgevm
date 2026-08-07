//! run_vm composes leases + spawn + reset cycles against real processes.

use super::*;
use std::io::Write;
use std::path::PathBuf;

struct Fixture {
    disk: PathBuf,
    vars: PathBuf,
    receipt: PathBuf,
    script: PathBuf,
}

/// A helper script that resets until the generation reaches `resets`.
fn fixture(tag: &str, resets: u32) -> Fixture {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let disk = dir.join(format!("bv-vmrun-{tag}-{pid}.raw"));
    let vars = dir.join(format!("bv-vmrun-{tag}-{pid}.fd"));
    let receipt = dir.join(format!("bv-vmrun-{tag}-{pid}.receipt"));
    let script = dir.join(format!("bv-vmrun-{tag}-{pid}.sh"));
    for stale in [&receipt, &script] {
        let _ = std::fs::remove_file(stale);
    }
    std::fs::write(&disk, b"disk").unwrap();
    std::fs::write(&vars, b"vars").unwrap();
    {
        let mut file = std::fs::File::create(&script).unwrap();
        writeln!(
            file,
            "#!/bin/sh\nif [ \"$BRIDGEVM_RESET_GENERATION\" -lt {resets} ]; then exit 42; fi\nexit 0"
        )
        .unwrap();
    }
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    Fixture {
        disk,
        vars,
        receipt,
        script,
    }
}

fn manifest(f: &Fixture) -> LaunchManifest {
    let text = format!(
        r#"{{"version": 1, "disk": "{}", "uefi_vars": "{}", "ram_mib": 2048, "vcpus": 2}}"#,
        f.disk.display(),
        f.vars.display()
    );
    LaunchManifest::parse(&text, false).unwrap()
}

fn launch(f: &Fixture) -> HelperLaunch {
    HelperLaunch {
        helper: f.script.clone(),
        firmware_code: "/fw/code.fd".into(),
        watchdog_ms: Some(1000),
        agent_control: None,
        surfaces: None,
        swtpm_sockets: None,
    }
}

fn cleanup(f: &Fixture) {
    for path in [&f.disk, &f.vars, &f.receipt, &f.script] {
        let _ = std::fs::remove_file(path);
    }
    for image in [&f.disk, &f.vars] {
        let mut lock = image.clone().into_os_string();
        lock.push(".bridgevm-writer.lock");
        let _ = std::fs::remove_file(PathBuf::from(lock));
    }
}

#[test]
fn three_generations_with_distinct_pids_and_a_final_receipt() {
    let f = fixture("cycles", 2);
    let cycles = run_vm(manifest(&f), &launch(&f), &f.receipt, 10, "vm_run test").unwrap();
    assert_eq!(cycles.len(), 3, "gen 0 reset, gen 1 reset, gen 2 shutdown");
    let generations: Vec<u64> = cycles.iter().map(|c| c.generation).collect();
    assert_eq!(generations, vec![0, 1, 2]);
    let mut pids: Vec<u32> = cycles.iter().map(|c| c.pid).collect();
    pids.dedup();
    assert_eq!(pids.len(), 3, "each generation must be a fresh process");
    // The receipt proves the LAST reset's flush (generation 1).
    let body = std::fs::read_to_string(&f.receipt).unwrap();
    assert!(body.contains("generation: 1"), "{body}");
    assert!(body.contains(&f.disk.display().to_string()), "{body}");
    cleanup(&f);
}

#[test]
fn a_competing_writer_fails_before_any_helper_runs() {
    let f = fixture("held", 0);
    let manifest_first = manifest(&f);
    let held = prepare(manifest_first, "first supervisor").unwrap();
    let error = run_vm(manifest(&f), &launch(&f), &f.receipt, 10, "second").unwrap_err();
    assert!(
        matches!(error, RuntimeError::MediaHeld { ref holder, .. } if holder.contains("first supervisor")),
        "{error}"
    );
    assert!(
        !f.receipt.exists(),
        "no helper may have run: the refusal is pre-launch"
    );
    drop(held);
    cleanup(&f);
}
