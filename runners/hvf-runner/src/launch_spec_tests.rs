use super::*;

fn spec_args() -> crate::Args {
    use clap::Parser;
    crate::Args::parse_from(["hvf-runner"])
}

fn write_manifest(tag: &str, body: &str) -> String {
    let path = std::env::temp_dir().join(format!("bv-spec-{}-{}", tag, std::process::id()));
    std::fs::write(&path, body).expect("write manifest");
    path.to_string_lossy().into_owned()
}

/// A disk/vars pair unique to one test: run_launch_spec now takes writer
/// leases, so two tests sharing /tmp/v.fd would race each other's flocks.
fn scratch_images(tag: &str) -> (String, String) {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let disk = dir.join(format!("bv-spec-{tag}-{pid}.raw"));
    let vars = dir.join(format!("bv-spec-{tag}-{pid}.fd"));
    for path in [&disk, &vars] {
        std::fs::write(path, b"image").expect("scratch image");
    }
    (
        disk.to_string_lossy().into_owned(),
        vars.to_string_lossy().into_owned(),
    )
}

#[test]
fn a_valid_manifest_file_is_accepted() {
    let (disk, vars) = scratch_images("ok");
    let path = write_manifest(
        "ok",
        &format!(
            r#"{{"version": 1, "disk": "{disk}", "uefi_vars": "{vars}",
            "ram_mib": 4096, "vcpus": 4}}"#
        ),
    );
    run_launch_spec(&path, &spec_args()).expect("valid manifest");
}

#[test]
fn a_rejected_manifest_names_the_reason() {
    let path = write_manifest(
        "badver",
        r#"{"version": 9, "disk": "/tmp/w.raw", "uefi_vars": "/tmp/v.fd",
            "ram_mib": 4096, "vcpus": 4}"#,
    );
    let error = run_launch_spec(&path, &spec_args()).expect_err("version 9");
    assert!(error.to_string().contains("version 9"), "{error}");
}

#[test]
fn a_missing_file_reports_the_path_not_a_panic() {
    let error = run_launch_spec("/nonexistent/spec.json", &spec_args()).expect_err("missing");
    assert!(
        error.to_string().contains("/nonexistent/spec.json"),
        "{error}"
    );
}

#[test]
fn tests_run_under_the_debug_policy_which_permits_repo_paths() {
    // The product/deviance split is compile-time; this suite is a debug
    // build, so a repo path must pass here. The release-mode refusal is
    // covered by manifest_tests in bridgevm-hvf-runtime, where the flag is
    // a parameter rather than cfg!().
    const { assert!(!PRODUCT_POLICY) };
    let (_, vars) = scratch_images("repo");
    let repo_disk = concat!(env!("CARGO_MANIFEST_DIR"), "/target-test-w.raw");
    std::fs::write(repo_disk, b"image").expect("repo scratch image");
    let path = write_manifest(
        "repo",
        &format!(
            r#"{{"version": 1, "disk": "{repo_disk}", "uefi_vars": "{vars}",
                "ram_mib": 4096, "vcpus": 4}}"#
        ),
    );
    run_launch_spec(&path, &spec_args()).expect("debug build accepts repo paths");
    let _ = std::fs::remove_file(repo_disk);
    let _ = std::fs::remove_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/target-test-w.raw.bridgevm-writer.lock"
    ));
}

#[test]
fn helper_mode_runs_generations_and_reports_them() {
    let (disk, vars) = scratch_images("run");
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let script = dir.join(format!("bv-spec-run-helper-{pid}.sh"));
    let receipt = dir.join(format!("bv-spec-run-{pid}.receipt"));
    let _ = std::fs::remove_file(&receipt);
    std::fs::write(
        &script,
        "#!/bin/sh\nif [ \"$BRIDGEVM_RESET_GENERATION\" -lt 1 ]; then exit 42; fi\nexit 0\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    let path = write_manifest(
        "run",
        &format!(
            r#"{{"version": 1, "disk": "{disk}", "uefi_vars": "{vars}",
            "ram_mib": 4096, "vcpus": 4}}"#
        ),
    );
    let mut args = spec_args();
    args.helper = Some(script.clone());
    args.supervise_receipt = Some(receipt.to_string_lossy().into_owned());
    run_launch_spec(&path, &args).expect("helper mode runs");
    // The receipt proves the generation-0 reset's flush.
    let body = std::fs::read_to_string(&receipt).unwrap();
    assert!(body.contains("generation: 0"), "{body}");
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_file(&receipt);
}
