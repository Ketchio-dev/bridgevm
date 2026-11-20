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
            fetch_drivers: true,
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
            "--fetch-drivers",
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
            fetch_drivers: false,
        }),
        library: None,
        json: false,
    };
    let forwarded = native_arguments(args);
    assert!(!forwarded.contains(&OsString::from("--no-network")));
    assert!(!forwarded.contains(&OsString::from("--fetch-drivers")));
    assert_eq!(forwarded.first(), Some(&OsString::from("--cli")));
}

#[test]
fn create_windows_fetch_drivers_flag_is_parsed_and_forwarded() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "app",
        "create-windows",
        "VM",
        "--iso",
        "/Windows.iso",
        "--fetch-drivers",
    ])
    .unwrap();
    let Command::App(args) = cli.command else {
        panic!("expected native app command")
    };
    assert_eq!(
        native_arguments(args).last(),
        Some(&OsString::from("--fetch-drivers"))
    );
}
