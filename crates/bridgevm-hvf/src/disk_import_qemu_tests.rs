//! Images made by qemu-img, when it is installed (CI's macOS runners install
//! it): the converter's output must be byte-identical to
//! `qemu-img convert -O raw` and keep the same holes.

use super::fixtures::{pattern, Scratch};
use super::*;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::process::Command;

fn qemu_tool(name: &str) -> Option<PathBuf> {
    ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
        .iter()
        .map(|dir| Path::new(dir).join(name))
        .find(|path| path.is_file())
}

fn run(tool: &Path, args: &[&str]) {
    let out = Command::new(tool).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{} {args:?}: {}",
        tool.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn allocated(path: &Path) -> u64 {
    fs::metadata(path).unwrap().blocks() * 512
}

/// A raw disk of 64 MiB + 1536 bytes: random data, explicit zeros, small
/// unaligned writes and a partial last cluster around holes.
fn source_raw(s: &Scratch) -> PathBuf {
    let path = s.path("source.raw");
    let file = File::create(&path).unwrap();
    file.set_len((64 << 20) + 1536).unwrap();
    for (offset, bytes) in [
        (0, pattern(1 << 20, 1)),
        (5 << 20, pattern(4096, 2)),
        (8 << 20, vec![0; 1 << 20]),
        ((20 << 20) + 512, pattern(1024, 3)),
        (33 << 20, pattern(3 << 20, 4)),
        ((64 << 20) - 4096, pattern(4096 + 1024, 5)),
    ] {
        file.write_all_at(&bytes, offset).unwrap();
    }
    path
}

/// Ours against qemu-img's raw conversion of the same image.
fn assert_converts_like_qemu(s: &Scratch, qemu_img: &Path, image: &Path, format: &str) {
    let reference = s.path("reference.raw");
    let ours = s.path("ours.raw");
    let _ = fs::remove_file(&reference);
    let _ = fs::remove_file(&ours);
    run(
        qemu_img,
        &[
            "convert",
            "-f",
            format,
            "-O",
            "raw",
            &image.display().to_string(),
            &reference.display().to_string(),
        ],
    );
    convert_to_raw(image, &ours, |_| {}).unwrap_or_else(|e| panic!("{}: {e}", image.display()));
    assert!(
        fs::read(&ours).unwrap() == fs::read(&reference).unwrap(),
        "{} differs from qemu-img",
        image.display()
    );
    assert!(
        allocated(&ours) <= allocated(&reference) + (64 << 10),
        "{}: {} bytes allocated, qemu-img {}",
        image.display(),
        allocated(&ours),
        allocated(&reference)
    );
}

#[test]
fn qemu_made_qcow2_and_vhdx_convert_byte_identical_to_qemu_img() {
    let (Some(qemu_img), Some(qemu_io)) = (qemu_tool("qemu-img"), qemu_tool("qemu-io")) else {
        eprintln!("qemu-img not installed: the hand-built fixtures cover the formats");
        return;
    };
    let s = Scratch::new("qemu");
    let raw = source_raw(&s);
    let raw_arg = raw.display().to_string();
    for (name, format, options) in [
        ("v2.qcow2", "qcow2", "compat=0.10"),
        ("v3.qcow2", "qcow2", "compat=1.1"),
        ("v3-512.qcow2", "qcow2", "compat=1.1,cluster_size=512"),
        ("v3-2m.qcow2", "qcow2", "compat=1.1,cluster_size=2M"),
        ("dynamic.vhdx", "vhdx", "subformat=dynamic"),
        ("dynamic-1m.vhdx", "vhdx", "subformat=dynamic,block_size=1M"),
        ("fixed.vhdx", "vhdx", "subformat=fixed"),
    ] {
        let image = s.path(name);
        run(
            &qemu_img,
            &[
                "convert",
                "-f",
                "raw",
                "-O",
                format,
                "-o",
                options,
                &raw_arg,
                &image.display().to_string(),
            ],
        );
        assert_converts_like_qemu(&s, &qemu_img, &image, format);
        assert!(
            fs::read(s.path("ours.raw")).unwrap() == fs::read(&raw).unwrap(),
            "{name}"
        );
    }

    // Guest-style writes after creation: data, v3 zero clusters (allocated
    // and not) over data, and a discard.
    for (name, format) in [("written.qcow2", "qcow2"), ("written.vhdx", "vhdx")] {
        let image = s.path(name);
        let arg = image.display().to_string();
        run(&qemu_img, &["create", "-f", format, &arg, "48M"]);
        run(
            &qemu_io,
            &[
                "-f",
                format,
                "-c",
                "write -P 0xab 0 4M",
                "-c",
                "write -P 0x5c 10M 64k",
                "-c",
                "write -z 1M 512k",
                "-c",
                "write -z -u 2M 64k",
                "-c",
                "discard 3M 64k",
                "-c",
                "write -P 0x11 40M 8M",
                &arg,
            ],
        );
        assert_converts_like_qemu(&s, &qemu_img, &image, format);
    }
}

#[test]
fn qemu_made_images_the_converter_refuses() {
    let Some(qemu_img) = qemu_tool("qemu-img") else {
        eprintln!("qemu-img not installed: the hand-built fixtures cover the refusals");
        return;
    };
    let s = Scratch::new("qemu-refused");
    let raw = source_raw(&s).display().to_string();
    let at = |name: &str| s.path(name).display().to_string();
    run(
        &qemu_img,
        &[
            "convert",
            "-f",
            "raw",
            "-O",
            "qcow2",
            "-c",
            &raw,
            &at("compressed.qcow2"),
        ],
    );
    run(
        &qemu_img,
        &["create", "-f", "qcow2", &at("base.qcow2"), "16M"],
    );
    run(
        &qemu_img,
        &[
            "create",
            "-f",
            "qcow2",
            "-b",
            &at("base.qcow2"),
            "-F",
            "qcow2",
            &at("overlay.qcow2"),
        ],
    );
    run(
        &qemu_img,
        &[
            "create",
            "-f",
            "qcow2",
            "-o",
            "extended_l2=on",
            &at("subclusters.qcow2"),
            "16M",
        ],
    );
    run(
        &qemu_img,
        &[
            "create",
            "-f",
            "qcow2",
            "-o",
            &format!("data_file={}", at("data.bin")),
            &at("external.qcow2"),
            "16M",
        ],
    );
    run(
        &qemu_img,
        &[
            "create",
            "-f",
            "qcow2",
            "--object",
            "secret,id=s0,data=bridgevm",
            "-o",
            "encrypt.format=luks,encrypt.key-secret=s0,encrypt.iter-time=10",
            &at("encrypted.qcow2"),
            "16M",
        ],
    );
    for (name, expected) in [
        ("compressed.qcow2", Refusal::Compressed),
        ("overlay.qcow2", Refusal::BackingFile),
        ("subclusters.qcow2", Refusal::ExtendedL2),
        ("external.qcow2", Refusal::ExternalDataFile),
        ("encrypted.qcow2", Refusal::Encrypted),
    ] {
        let dst = s.path(&format!("{name}.raw"));
        match convert_to_raw(&s.path(name), &dst, |_| {}) {
            Err(ConvertError::Refused(refusal)) => assert_eq!(refusal, expected, "{name}"),
            other => panic!("{name}: {other:?}"),
        }
        assert!(!dst.exists());
    }
}
