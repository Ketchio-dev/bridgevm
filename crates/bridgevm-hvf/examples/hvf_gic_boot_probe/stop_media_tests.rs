use super::*;
use bridgevm_hvf::snapshot_pair::managed::runtime::acquire;
use std::fs;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "bridgevm-stop-media-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn media(&self, name: &str) -> WritableMedia {
        let path = self.0.join(name);
        fs::write(&path, b"initial").unwrap();
        WritableMedia::new(path).with_write_back(true)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn stop_records_are_the_completed_writes_in_order() {
    let scratch = Scratch::new();
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.flash_vars = scratch.media("vars");
    media.nvme_disk = Some(scratch.media("primary"));
    media.nvme_target = Some(scratch.media("target"));
    let mut owner = acquire(&mut media).unwrap();
    let mut platform = VirtPlatform::new(VirtFdtConfig::default());
    platform.load_flash_vars(b"saved-vars");
    platform.load_nvme_disk(vec![0x31; 512]);
    platform.attach_nvme_second_namespace(512);

    let records = persist_stop_media(&mut platform, &media, &mut owner);

    let path = |slot: Option<&WritableMedia>| slot.unwrap().path.display().to_string();
    let vars_len = platform.flash_vars_image().len();
    assert_eq!(
        records,
        [
            format!(
                "host media: UEFI vars written back: {} ({vars_len} bytes)",
                path(Some(&media.flash_vars))
            ),
            format!(
                "host media: NVMe disk written back: {} (512 bytes)",
                path(media.nvme_disk.as_ref())
            ),
            format!(
                "host media: NVMe target namespace (NSID 2) written back: {} (512 bytes)",
                path(media.nvme_target.as_ref())
            ),
        ]
    );
}

#[test]
fn a_record_keeps_its_host_path_on_one_ascii_line() {
    let write = |kind, path: &str, bytes| MediaWrite {
        kind,
        path: PathBuf::from(path),
        bytes,
    };
    let odd = "/tmp/a\nstop: PSCI 0x84000008 (system off)/\u{d55c}.raw";

    assert_eq!(
        report_media_writes("NVMe disk", &[write(MediaWriteKind::WriteBack, odd, 4096)]),
        ["host media: NVMe disk written back: \
             /tmp/a\\x0astop: PSCI 0x84000008 (system off)/\\xed\\x95\\x9c.raw (4096 bytes)"]
    );
    assert_eq!(
        report_media_writes(
            "NVMe disk",
            &[write(MediaWriteKind::Snapshot, "/tmp/out.raw", 1)]
        ),
        ["host media: NVMe disk snapshot written: /tmp/out.raw (1 bytes)"]
    );
}

#[test]
fn final_report_prints_the_records_right_after_its_stop_record() {
    let source: Vec<&str> = include_str!("final_report.rs")
        .lines()
        .map(str::trim)
        .collect();
    let stop = source
        .iter()
        .position(|line| *line == r#"println!("stop: {}", $stop_reason);"#)
        .unwrap();

    assert_eq!(
        source[stop - 1],
        r#"println!("=== EDK2 boot probe (with Apple hv_gic) ===");"#
    );
    assert_eq!(
        source[stop + 1],
        r#"for record in &host_media { println!("{record}"); }"#
    );
    let persisted = source
        .iter()
        .position(|line| line.starts_with("let host_media = persist_stop_media("))
        .unwrap();
    assert!(persisted < stop);
}
