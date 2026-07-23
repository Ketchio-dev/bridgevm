//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn print_vm_record(vm: &VmRecord) {
    println!(
        "{}\t{}\t{}\t{} {}\t{}",
        vm.name,
        vm.state,
        vm.mode,
        vm.guest_os,
        vm.guest_arch,
        vm.path.display()
    );
    if let Some(supervisor) = &vm.qmp_supervisor {
        print_qmp_supervisor(supervisor);
    }
}

pub(crate) fn list(store: &VmStore) -> Result<()> {
    let vms = store.list_vms().context("failed to list VMs")?;
    if vms.is_empty() {
        println!("No VMs found in {}", store.vms_dir().display());
        return Ok(());
    }
    for (path, manifest) in vms {
        let state = store
            .state(&manifest.name)
            .map(|metadata| metadata.state.to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        println!(
            "{}\t{}\t{}\t{} {}\t{}",
            manifest.name,
            state,
            manifest.mode,
            manifest.guest.os,
            manifest.guest.arch,
            path.display()
        );
    }
    Ok(())
}

pub(crate) fn templates() -> Result<()> {
    let templates = available_boot_templates();
    print_boot_templates(&templates);
    Ok(())
}

pub(crate) fn create(store: &VmStore, args: CreateArgs) -> Result<()> {
    let manifest = manifest_for_create(args)?;
    let rec = recommend_mode(&GuestChoice {
        os: manifest.guest.os.clone(),
        version: manifest.guest.version.clone(),
        arch: manifest.guest.arch.clone(),
    });
    let path = store
        .create_vm(&manifest)
        .context("failed to create VM bundle")?;
    println!("Created {} VM at {}", manifest.mode, path.display());
    println!("{}", rec.message);
    Ok(())
}

pub(crate) fn manifest_for_create(args: CreateArgs) -> Result<VmManifest> {
    let template = args
        .template
        .as_deref()
        .map(|id| boot_template_by_id(id).with_context(|| format!("unknown template id: {id}")))
        .transpose()?;
    let os = args
        .os
        .clone()
        .or_else(|| template.as_ref().map(|template| template.guest_os.clone()))
        .context("create requires --os unless --template provides a guest")?;
    let version = args.version.clone().or_else(|| {
        template
            .as_ref()
            .and_then(|template| template.guest_version.clone())
    });
    let arch = args
        .arch
        .clone()
        .or_else(|| {
            template
                .as_ref()
                .map(|template| template.guest_arch.clone())
        })
        .unwrap_or_else(|| "arm64".to_string());
    let choice = GuestChoice {
        os: os.clone(),
        version: version.clone(),
        arch: arch.clone(),
    };
    let rec = recommend_mode(&choice);
    let mode = match args.mode {
        ModeChoice::Auto => rec.mode,
        ModeChoice::Fast if rec.fast_mode_available => VmMode::Fast,
        ModeChoice::Fast => bail!("{}", rec.message),
        ModeChoice::Compatibility => VmMode::Compatibility,
    };

    let disk_size = args
        .disk
        .clone()
        .or_else(|| {
            template
                .as_ref()
                .and_then(|template| template.primary_disk_size().map(str::to_string))
        })
        .unwrap_or_else(|| DEFAULT_PRIMARY_DISK_SIZE.to_string());
    let boot = boot_for_create(&args, mode, &rec, template.as_ref());
    let mut manifest = VmManifest::new(args.name, mode, Guest { os, version, arch }, disk_size);
    if let Some(template) = &template {
        template.apply_storage_defaults(&mut manifest.storage.primary);
        if let Some(disk) = &args.disk {
            manifest.storage.primary.size = disk.clone();
        }
    }
    if let Some(disk_format) = args.disk_format {
        manifest.storage.primary.format = disk_format.manifest_format().to_string();
        manifest.storage.primary.path = disk_format.default_primary_path().to_string();
    }
    manifest.boot = Some(boot);
    Ok(manifest)
}

pub(crate) fn boot_for_create(
    args: &CreateArgs,
    mode: VmMode,
    rec: &ModeRecommendation,
    template: Option<&BootTemplate>,
) -> Boot {
    let explicit_boot = args.boot_mode.is_some()
        || args.installer_image.is_some()
        || args.kernel_path.is_some()
        || args.initrd_path.is_some()
        || args.kernel_command_line.is_some()
        || args.macos_restore_image.is_some();
    if !explicit_boot && mode == VmMode::Fast {
        if let Some(template) = template {
            return template.as_boot();
        }
        if let Some(template) = &rec.boot_template {
            return template.as_boot();
        }
    }

    let inferred_boot_mode = args
        .boot_mode
        .map(BootMode::from)
        .or_else(|| {
            args.installer_image
                .as_ref()
                .map(|_| BootMode::LinuxInstaller)
        })
        .or_else(|| args.kernel_path.as_ref().map(|_| BootMode::LinuxKernel))
        .or_else(|| {
            args.macos_restore_image
                .as_ref()
                .map(|_| BootMode::MacosRestore)
        })
        .unwrap_or(BootMode::ExistingDisk);
    Boot {
        mode: inferred_boot_mode,
        installer_image: args.installer_image.clone(),
        kernel_path: args.kernel_path.clone(),
        initrd_path: args.initrd_path.clone(),
        kernel_command_line: args.kernel_command_line.clone(),
        macos_restore_image: args.macos_restore_image.clone(),
    }
}

pub(crate) fn status(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let (_, manifest) = store.get_vm(&args.name).context("failed to read VM")?;
    let state = store.state(&args.name).context("failed to read VM state")?;
    println!("Name: {}", manifest.name);
    println!("Mode: {}", manifest.mode);
    println!("Guest: {} {}", manifest.guest.os, manifest.guest.arch);
    println!("State: {}", state.state);
    println!("Updated: {}", state.updated_at_unix);
    Ok(())
}

pub(crate) fn transition(
    store: &VmStore,
    args: VmNameArgs,
    to: VmRuntimeState,
    verb: &str,
) -> Result<()> {
    let state = store
        .transition_state(&args.name, to)
        .with_context(|| format!("failed to transition VM '{}'", args.name))?;
    println!("{} {} ({})", verb, args.name, state.state);
    Ok(())
}

pub(crate) fn delete(store: &VmStore, args: DeleteArgs) -> Result<()> {
    let state = store.state(&args.name).context("failed to read VM state")?;
    if state.state == VmRuntimeState::Running {
        bail!("refusing to delete a running VM; stop it first");
    }
    if args.metadata_only {
        let metadata = store
            .delete_vm_metadata_only(&args.name)
            .with_context(|| format!("failed to delete VM metadata '{}'", args.name))?;
        println!(
            "Deleted VM metadata for {} at {} (bundle preserved: {})",
            metadata.vm,
            metadata.metadata_path.display(),
            metadata.bundle.display()
        );
        return Ok(());
    }
    let path = store
        .delete_vm(&args.name)
        .with_context(|| format!("failed to delete VM '{}'", args.name))?;
    println!("Deleted VM bundle {}", path.display());
    Ok(())
}

pub(crate) fn export_vm(store: &VmStore, args: ExportArgs) -> Result<()> {
    let export = store
        .export_vm(&args.name, &args.output)
        .with_context(|| format!("failed to export VM '{}'", args.name))?;
    println!(
        "Exported {} from {} to {}",
        export.vm,
        export.source.display(),
        export.output.display()
    );
    Ok(())
}

pub(crate) fn import_vm(store: &VmStore, args: ImportArgs) -> Result<()> {
    let import = store
        .import_vm(&args.input, args.name.as_deref())
        .with_context(|| format!("failed to import VM bundle '{}'", args.input.display()))?;
    println!(
        "Imported {} from {} to {}",
        import.vm,
        import.source.display(),
        import.output.display()
    );
    Ok(())
}

pub(crate) fn clone_vm(store: &VmStore, args: CloneArgs) -> Result<()> {
    let clone = store
        .clone_vm(&args.name, &args.new_name, args.linked)
        .with_context(|| format!("failed to clone VM '{}'", args.name))?;
    print_clone(&clone);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_store(prefix: &str) -> VmStore {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        VmStore::new(root)
    }

    fn unique_trace_path(prefix: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "{prefix}-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    }

    fn test_manifest(name: &str) -> VmManifest {
        VmManifest::new(
            name,
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        )
    }

    fn create_args_for_windows_11_arm(name: &str) -> CreateArgs {
        CreateArgs {
            name: name.to_string(),
            template: None,
            os: Some("windows".to_string()),
            version: Some("11".to_string()),
            arch: Some("arm64".to_string()),
            mode: ModeChoice::Auto,
            disk: Some("128GiB".to_string()),
            disk_format: Some(DiskFormatChoice::Qcow2),
            boot_mode: None,
            installer_image: None,
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        }
    }

    #[test]
    fn create_auto_uses_compatibility_for_windows_11_arm() {
        let manifest =
            manifest_for_create(create_args_for_windows_11_arm("win11")).expect("manifest");

        assert_eq!(manifest.mode, VmMode::Compatibility);
        assert_eq!(manifest.guest.os, "windows");
        assert_eq!(manifest.guest.version.as_deref(), Some("11"));
        assert_eq!(manifest.guest.arch, "arm64");
    }

    #[test]
    fn create_rejects_explicit_fast_mode_for_windows_11_arm() {
        let mut args = create_args_for_windows_11_arm("win11");
        args.mode = ModeChoice::Fast;

        let error = manifest_for_create(args).expect_err("Windows should not be Fast Mode");
        assert!(error
            .to_string()
            .contains("Apple VZ Fast Mode is Linux/macOS Arm only"));
    }

    #[test]
    fn create_accepts_raw_disk_format_for_fast_linux_kernel_live_path() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "create",
            "vz-linux",
            "--os",
            "ubuntu",
            "--arch",
            "arm64",
            "--mode",
            "fast",
            "--boot-mode",
            "linux-kernel",
            "--kernel-path",
            "boot/vmlinuz",
            "--initrd-path",
            "boot/initrd",
            "--kernel-command-line",
            "console=hvc0 root=/dev/vda",
            "--disk",
            "64MiB",
            "--disk-format",
            "raw",
        ])
        .unwrap();
        let Command::Create(args) = cli.command else {
            panic!("expected create command");
        };

        let manifest = manifest_for_create(args).expect("manifest");
        assert_eq!(manifest.mode, VmMode::Fast);
        assert_eq!(manifest.storage.primary.path, "disks/root.raw");
        assert_eq!(manifest.storage.primary.format, "raw");
        assert_eq!(manifest.storage.primary.size, "64MiB");
        let boot = manifest.boot.expect("boot");
        assert_eq!(boot.mode, BootMode::LinuxKernel);
        assert_eq!(boot.kernel_path.as_deref(), Some("boot/vmlinuz"));
        assert_eq!(boot.initrd_path.as_deref(), Some("boot/initrd"));
        assert_eq!(
            boot.kernel_command_line.as_deref(),
            Some("console=hvc0 root=/dev/vda")
        );
    }

    #[test]
    fn create_uses_debian_apple_vz_linux_kernel_raw_template_storage() {
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

        let manifest = manifest_for_create(args).expect("manifest");
        assert_eq!(manifest.mode, VmMode::Fast);
        assert_eq!(manifest.guest.os, "debian");
        assert_eq!(manifest.guest.arch, "arm64");
        assert_eq!(manifest.storage.primary.path, "disks/root.raw");
        assert_eq!(manifest.storage.primary.format, "raw");
        assert_eq!(manifest.storage.primary.size, "64MiB");
        let boot = manifest.boot.expect("boot");
        assert_eq!(boot.mode, BootMode::LinuxKernel);
        assert_eq!(boot.kernel_path.as_deref(), Some("boot/vmlinuz"));
        assert_eq!(boot.initrd_path.as_deref(), Some("boot/initrd"));
        assert_eq!(
            boot.kernel_command_line.as_deref(),
            Some("console=hvc0 priority=low")
        );
    }

    #[test]
    fn create_uses_ubuntu_apple_vz_linux_kernel_raw_template_storage() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "create",
            "ubuntu-desktop-vz",
            "--template",
            "ubuntu-arm64-apple-vz-linux-kernel-raw",
        ])
        .unwrap();
        let Command::Create(args) = cli.command else {
            panic!("expected create command");
        };

        let manifest = manifest_for_create(args).expect("manifest");
        assert_eq!(manifest.mode, VmMode::Fast);
        assert_eq!(manifest.guest.os, "ubuntu");
        assert_eq!(manifest.guest.arch, "arm64");
        assert_eq!(manifest.storage.primary.path, "disks/root.raw");
        assert_eq!(manifest.storage.primary.format, "raw");
        assert_eq!(manifest.storage.primary.size, "32GiB");
        let boot = manifest.boot.expect("boot");
        assert_eq!(boot.mode, BootMode::LinuxKernel);
        assert_eq!(boot.kernel_path.as_deref(), Some("boot/vmlinuz"));
        assert_eq!(boot.initrd_path.as_deref(), Some("boot/initrd"));
        assert_eq!(
            boot.kernel_command_line.as_deref(),
            Some("console=hvc0 root=/dev/vda2 rw systemd.unit=graphical.target")
        );
    }

    #[test]
    fn virtio_gpu_trace_report_flags_missing_submit_and_fence() {
        let path = unique_trace_path("bridgevm-cli-virtio-gpu-missing");
        fs::write(
            &path,
            r#"{"seq":1,"event":"device_init","backend_3d":true}
{"seq":2,"event":"driver_features","select":0,"accepted":8}
{"seq":3,"event":"driver_features","select":1,"accepted":1}
{"seq":4,"event":"queue_notify","valid":true}
{"seq":5,"event":"command","name":"GET_CAPSET_INFO","response_name":"OK_CAPSET_INFO","response_capset_id":4,"response_capset_max_version":1,"response_capset_max_size":64}
{"seq":6,"event":"command","name":"GET_CAPSET","response_name":"OK_CAPSET","capset_id":4,"capset_version":1}
{"seq":7,"event":"command","name":"RESOURCE_CREATE_BLOB","response_name":"OK_NODATA"}
{"seq":8,"event":"command","name":"CTX_CREATE","response_name":"OK_NODATA","context_init":4}
"#,
        )
        .unwrap();

        let report = analyze_virtio_gpu_trace(&path).unwrap();
        let _ = fs::remove_file(path);
        let blockers = report.p3_blockers(VirtioGpuTraceProtocolChoice::Auto);

        assert!(blockers
            .iter()
            .any(|blocker| blocker == "missing successful SUBMIT_3D"));
        assert!(blockers.iter().any(|blocker| {
            blocker == "missing fenced command plus fence create/completion/delivery"
        }));
    }

    #[test]
    fn local_export_error_keeps_storage_reason() {
        let store = unique_store("bridgevm-cli-export-hardening-test");
        let bundle = store.create_vm(&test_manifest("dev")).unwrap();

        let error = export_vm(
            &store,
            ExportArgs {
                name: "dev".to_string(),
                output: bundle.join("nested").join("dev.vmbridge"),
            },
        )
        .unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("failed to export VM 'dev'"),
            "missing CLI context: {message}"
        );
        assert!(
            message.contains("export output must not be the source bundle or inside it"),
            "missing storage reason: {message}"
        );
    }

    #[test]
    fn local_import_error_keeps_storage_reason() {
        let store = unique_store("bridgevm-cli-import-hardening-test");
        let bundle = store.create_vm(&test_manifest("dev")).unwrap();

        let error = import_vm(
            &store,
            ImportArgs {
                input: bundle,
                name: None,
            },
        )
        .unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("failed to import VM bundle"),
            "missing CLI context: {message}"
        );
        assert!(
            message.contains("import input conflicts with the destination store"),
            "missing storage reason: {message}"
        );
    }
}
