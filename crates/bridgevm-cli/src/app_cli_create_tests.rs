use super::*;

#[test]
fn create_windows_forwards_one_exact_request_without_a_shell() {
    let args = AppArgs {
        command: AppCommand::CreateWindows(AppCreateWindowsArgs {
            name: "개발 VM 'A'".into(),
            iso: PathBuf::from("/owned media/Windows 11.iso"),
            disk_gib: 128,
            memory_mib: 8_192,
            cpus: 6,
            resolution: "1920x1080".into(),
            no_network: true,
        }),
        library: Some(PathBuf::from("/owned library")),
        json: true,
    };
    assert_eq!(
        native_arguments(args),
        [
            "--cli",
            "create-windows",
            "개발 VM 'A'",
            "--iso",
            "/owned media/Windows 11.iso",
            "--disk-gib",
            "128",
            "--memory-mib",
            "8192",
            "--cpus",
            "6",
            "--resolution",
            "1920x1080",
            "--no-network",
            "--library",
            "/owned library",
            "--json",
        ]
        .map(OsString::from)
    );
}

#[test]
fn create_windows_default_network_adds_no_disable_flag() {
    let args = AppArgs {
        command: AppCommand::CreateWindows(AppCreateWindowsArgs {
            name: "VM".into(),
            iso: PathBuf::from("/Windows.iso"),
            disk_gib: 64,
            memory_mib: 6_144,
            cpus: 4,
            resolution: "1440x900".into(),
            no_network: false,
        }),
        library: None,
        json: false,
    };
    let forwarded = native_arguments(args);
    assert!(!forwarded.contains(&OsString::from("--no-network")));
    assert_eq!(forwarded.first(), Some(&OsString::from("--cli")));
}
