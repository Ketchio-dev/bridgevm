use super::*;

fn write_manifest(tag: &str, body: &str) -> String {
    let path = std::env::temp_dir().join(format!("bv-spec-{}-{}", tag, std::process::id()));
    std::fs::write(&path, body).expect("write manifest");
    path.to_string_lossy().into_owned()
}

#[test]
fn a_valid_manifest_file_is_accepted() {
    let path = write_manifest(
        "ok",
        r#"{"version": 1, "disk": "/tmp/w.raw", "uefi_vars": "/tmp/v.fd",
            "ram_mib": 4096, "vcpus": 4}"#,
    );
    run_launch_spec(&path).expect("valid manifest");
}

#[test]
fn a_rejected_manifest_names_the_reason() {
    let path = write_manifest(
        "badver",
        r#"{"version": 9, "disk": "/tmp/w.raw", "uefi_vars": "/tmp/v.fd",
            "ram_mib": 4096, "vcpus": 4}"#,
    );
    let error = run_launch_spec(&path).expect_err("version 9");
    assert!(error.to_string().contains("version 9"), "{error}");
}

#[test]
fn a_missing_file_reports_the_path_not_a_panic() {
    let error = run_launch_spec("/nonexistent/spec.json").expect_err("missing");
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
    let repo_disk = concat!(env!("CARGO_MANIFEST_DIR"), "/w.raw");
    let path = write_manifest(
        "repo",
        &format!(
            r#"{{"version": 1, "disk": "{repo_disk}", "uefi_vars": "/tmp/v.fd",
                "ram_mib": 4096, "vcpus": 4}}"#
        ),
    );
    run_launch_spec(&path).expect("debug build accepts repo paths");
}
