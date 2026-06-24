use super::*;

#[test]
fn ramfb_checkpoint_writes_distinct_artifacts_for_repeated_label() {
    // Given: a deterministic one-pixel XRGB8888 framebuffer and an empty artifact dir.
    let dir = checkpoint_test_dir();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let config = RamfbConfig {
        addr: 0x4008_0000,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 1,
        height: 1,
        stride: 4,
    };
    let ram = TestRam {
        base: config.addr,
        bytes: vec![0x03, 0x02, 0x01, 0x00],
    };
    let _print_checkpoint: fn(&str, Option<RamfbConfig>, &dyn GuestMemoryMut) = print_checkpoint;

    // When: the same checkpoint label is emitted twice.
    let first = RamfbCheckpoint::new("setup-input-before", Some(config), &ram)
        .emit(Some(&dir))
        .unwrap();
    let second = RamfbCheckpoint::new("setup-input-before", Some(config), &ram)
        .emit(Some(&dir))
        .unwrap();

    // Then: each summary is parseable and points at a distinct raw and PPM artifact.
    println!("{}", first.line);
    println!("{}", second.line);
    let first_paths = first.paths.unwrap();
    let second_paths = second.paths.unwrap();
    assert_ne!(first_paths.raw, second_paths.raw);
    assert_ne!(first_paths.ppm, second_paths.ppm);
    assert!(first.line.contains("label=setup-input-before"));
    assert!(first.line.contains("state=captured"));
    assert!(first.line.contains("checksum64=0x"));
    assert!(first.line.contains("raw="));
    assert!(first.line.contains("ppm="));
    assert_eq!(
        first_paths.raw.file_name().unwrap(),
        "ramfb-checkpoint-setup-input-before-0000.xrgb8888"
    );
    assert_eq!(
        second_paths.raw.file_name().unwrap(),
        "ramfb-checkpoint-setup-input-before-0001.xrgb8888"
    );
    assert_eq!(std::fs::read(first_paths.raw).unwrap(), ram.bytes);
    assert_eq!(
        std::fs::read(second_paths.ppm).unwrap(),
        b"P6\n1 1\n255\n\x01\x02\x03"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn ramfb_checkpoint_marks_inactive_with_machine_friendly_checksum() {
    // Given: no RAMFB config is available.
    let ram = TestRam {
        base: 0x4008_0000,
        bytes: Vec::new(),
    };

    // When: a checkpoint is emitted.
    let record = RamfbCheckpoint::new("setup-input-before", None, &ram)
        .emit(None)
        .unwrap();

    // Then: the line stays parseable and uses a machine-friendly checksum token.
    println!("{}", record.line);
    assert_eq!(field_value(&record.line, "state"), "inactive");
    assert_eq!(field_value(&record.line, "checksum64"), "none");
    assert_eq!(field_value(&record.line, "raw"), "none");
    assert_eq!(field_value(&record.line, "ppm"), "none");
}

#[test]
fn ramfb_checkpoint_marks_unavailable_with_machine_friendly_checksum() {
    // Given: RAMFB points outside the available test memory.
    let config = RamfbConfig {
        addr: 0x4008_0000,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 1,
        height: 1,
        stride: 4,
    };
    let ram = TestRam {
        base: config.addr + 0x1000,
        bytes: vec![0; 4],
    };

    // When: a checkpoint is emitted.
    let record = RamfbCheckpoint::new("setup-input-before", Some(config), &ram)
        .emit(None)
        .unwrap();

    // Then: the line stays parseable and separates the error from checksum64.
    println!("{}", record.line);
    assert_eq!(field_value(&record.line, "state"), "unavailable");
    assert_eq!(field_value(&record.line, "checksum64"), "none");
    assert_ne!(field_value(&record.line, "error"), "none");
    assert!(!field_value(&record.line, "checksum64").contains('{'));
}

#[test]
fn ramfb_sample_checkpoint_lines_parse_when_inactive_or_unavailable() {
    // Given: sample labels are independent of setup-input marker presence.
    let inactive_ram = TestRam {
        base: 0x4008_0000,
        bytes: Vec::new(),
    };
    let unavailable_config = RamfbConfig {
        addr: 0x4008_0000,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 1,
        height: 1,
        stride: 4,
    };
    let unavailable_ram = TestRam {
        base: unavailable_config.addr + 0x1000,
        bytes: vec![0; 4],
    };

    // When: sample checkpoints are emitted without active/capturable RAMFB.
    let inactive = RamfbCheckpoint::new("ramfb-sample-1000ms", None, &inactive_ram)
        .emit(None)
        .unwrap();
    let unavailable = RamfbCheckpoint::new(
        "ramfb-sample-1000ms",
        Some(unavailable_config),
        &unavailable_ram,
    )
    .emit(None)
    .unwrap();

    // Then: both lines remain parseable and fail closed via explicit state.
    println!("{}", inactive.line);
    println!("{}", unavailable.line);
    assert_eq!(field_value(&inactive.line, "label"), "ramfb-sample-1000ms");
    assert_eq!(field_value(&inactive.line, "state"), "inactive");
    assert_eq!(
        field_value(&unavailable.line, "label"),
        "ramfb-sample-1000ms"
    );
    assert_eq!(field_value(&unavailable.line, "state"), "unavailable");
}
