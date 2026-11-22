//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;

#[test]
fn create_keeps_qcow2_defaults_without_template_storage() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "create",
        "plain-linux",
        "--os",
        "ubuntu",
        "--arch",
        "arm64",
    ])
    .unwrap();
    let Command::Create(args) = cli.command else {
        panic!("expected create command");
    };

    let manifest = manifest_for_create(args).expect("manifest");
    assert_eq!(manifest.mode, VmMode::Fast);
    assert_eq!(manifest.storage.primary.path, "disks/root.qcow2");
    assert_eq!(manifest.storage.primary.format, "qcow2");
    assert_eq!(manifest.storage.primary.size, DEFAULT_PRIMARY_DISK_SIZE);
}

#[test]
fn socket_request_for_plain_template_create_uses_daemon_template_api() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "create",
        "try-vz-linux",
        "--template",
        "debian-arm64-apple-vz-linux-kernel-raw",
    ])
    .unwrap();
    let Command::Create(args) = cli.command else {
        panic!("expected create command");
    };

    let request = request_for(Command::Create(args)).expect("request");
    let BridgeVmRequest::CreateVmFromTemplate { name, template_id } = request else {
        panic!("expected create-from-template request");
    };
    assert_eq!(name, "try-vz-linux");
    assert_eq!(template_id, "debian-arm64-apple-vz-linux-kernel-raw");
}
