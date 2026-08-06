use super::*;

fn valid() -> String {
    r#"{"version": 1, "disk": "/tmp/win.raw", "uefi_vars": "/tmp/vars.fd",
        "ram_mib": 8192, "vcpus": 4}"#
        .to_string()
}

#[test]
fn a_valid_manifest_parses_to_its_fields() {
    let m = LaunchManifest::parse(&valid(), false).expect("valid");
    assert_eq!(m.disk(), "/tmp/win.raw");
    assert_eq!(m.uefi_vars(), "/tmp/vars.fd");
    assert_eq!(m.ram_mib(), 8192);
    assert_eq!(m.vcpus(), 4);
}

#[test]
fn an_unknown_version_is_refused_not_guessed_at() {
    let text = valid().replace("\"version\": 1", "\"version\": 2");
    match LaunchManifest::parse(&text, false) {
        Err(RuntimeError::UnsupportedManifestVersion { found: 2 }) => {}
        other => panic!("expected version refusal, got {other:?}"),
    }
}

#[test]
fn each_missing_field_is_named() {
    for field in ["version", "disk", "uefi_vars", "ram_mib", "vcpus"] {
        let text = valid().replace(&format!("\"{field}\""), "\"gone\"");
        match LaunchManifest::parse(&text, false) {
            Err(RuntimeError::ManifestField {
                field: named,
                problem: "missing",
            }) => {
                assert_eq!(named, field);
            }
            Err(RuntimeError::UnsupportedManifestVersion { .. }) if field == "version" => {
                panic!("a missing version must be 'missing', not a version mismatch")
            }
            other => panic!("expected missing {field}, got {other:?}"),
        }
    }
}

#[test]
fn out_of_range_sizing_is_refused() {
    let too_small = valid().replace("8192", "512");
    assert!(matches!(
        LaunchManifest::parse(&too_small, false),
        Err(RuntimeError::ManifestField {
            field: "ram_mib",
            ..
        })
    ));
    let too_many = valid().replace("\"vcpus\": 4", "\"vcpus\": 65");
    assert!(matches!(
        LaunchManifest::parse(&too_many, false),
        Err(RuntimeError::ManifestField { field: "vcpus", .. })
    ));
}

#[test]
fn disk_and_vars_may_not_be_the_same_file() {
    let text = valid().replace("/tmp/vars.fd", "/tmp/win.raw");
    assert!(matches!(
        LaunchManifest::parse(&text, false),
        Err(RuntimeError::DuplicateDiskWriter { .. })
    ));
}

#[test]
fn a_product_launch_refuses_a_repository_path() {
    // This test runs inside the bridgevm repo, so the repo's own .git is the
    // marker; a scratch dir under /tmp has none.
    let repo_disk = concat!(env!("CARGO_MANIFEST_DIR"), "/win.raw");
    let text = valid().replace("/tmp/win.raw", repo_disk);
    match LaunchManifest::parse(&text, true) {
        Err(RuntimeError::RepositoryPathInProduct { field: "disk" }) => {}
        other => panic!("expected repo-path refusal, got {other:?}"),
    }
    // The same manifest is fine for an evidence harness.
    assert!(LaunchManifest::parse(&text, false).is_ok());
}

#[test]
fn escapes_and_non_string_paths_are_refused() {
    let escaped = valid().replace("/tmp/win.raw", "C:\\\\win.raw");
    assert!(matches!(
        LaunchManifest::parse(&escaped, false),
        Err(RuntimeError::ManifestField { field: "disk", .. })
    ));
    let numeric = valid().replace("\"/tmp/win.raw\"", "7");
    assert!(matches!(
        LaunchManifest::parse(&numeric, false),
        Err(RuntimeError::ManifestField { field: "disk", .. })
    ));
}

#[test]
fn a_comma_inside_a_string_does_not_end_the_field() {
    let text = valid().replace("/tmp/win.raw", "/tmp/a,b.raw");
    let m = LaunchManifest::parse(&text, false).expect("comma in string");
    assert_eq!(m.disk(), "/tmp/a,b.raw");
}
