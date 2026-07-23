//! The mode-driven factory that fills a new manifest with Fast or Compatibility defaults.

use crate::*;

impl VmManifest {
    pub fn new(
        name: impl Into<String>,
        mode: VmMode,
        guest: Guest,
        disk_size: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let disk_size = disk_size.into();
        let fast = mode == VmMode::Fast;
        let hostname = format!("{}.bridgevm.local", slug(&name));

        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            name,
            mode,
            guest,
            backend: if fast {
                Backend {
                    engine: "lightvm".to_string(),
                    preferred: Some("apple-vz".to_string()),
                    fallback: Some("qemu-hvf-restricted".to_string()),
                    accelerator: Some("hvf".to_string()),
                }
            } else {
                Backend {
                    engine: "qemu".to_string(),
                    preferred: None,
                    fallback: Some("tcg".to_string()),
                    accelerator: Some("hvf".to_string()),
                }
            },
            resources: Resources {
                profile: "automatic".to_string(),
                memory: "auto".to_string(),
                cpu: "auto".to_string(),
            },
            display: Display {
                renderer: if fast { "metal" } else { "spice-or-vnc" }.to_string(),
                frame_policy: "adaptive".to_string(),
                retina: true,
            },
            storage: Storage {
                primary: PrimaryDisk {
                    path: "disks/root.qcow2".to_string(),
                    size: disk_size,
                    format: "qcow2".to_string(),
                    discard: fast,
                },
            },
            boot: Some(Boot {
                mode: BootMode::ExistingDisk,
                installer_image: None,
                kernel_path: None,
                initrd_path: None,
                kernel_command_line: None,
                macos_restore_image: None,
            }),
            network: Network {
                mode: "nat".to_string(),
                hostname,
                forwards: Vec::new(),
                bridge_interface: None,
            },
            integration: Integration {
                tools: if fast { "required" } else { "optional" }.to_string(),
                clipboard: true,
                drag_drop: fast,
                dynamic_resolution: true,
                shared_folders: true,
                applications: true,
                windows: true,
            },
            security: Security {
                shared_folder_approval: "required".to_string(),
                guest_command_execution: false,
                signed_agent_updates: true,
            },
            shared_folders: Vec::new(),
            firmware: Firmware::default(),
        }
    }
}
