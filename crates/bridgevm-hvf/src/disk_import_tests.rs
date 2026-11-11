use super::fixtures::{
    patch_be, pattern, qcow2, qcow2_field as q, seal, vhdx, Block, Cluster, Scratch, VhdxSpec, MIB,
    VHDX_BAT, VHDX_HEADERS, VHDX_METADATA, VHDX_REGIONS,
};
use super::*;
use std::fs;
use std::os::unix::fs::MetadataExt;

/// Convert `image` and return the raw bytes, after checking that nothing
/// but the result is left in the directory and that progress rose to 100.
fn convert(s: &Scratch, image: &[u8]) -> std::result::Result<Vec<u8>, ConvertError> {
    let src = s.write("src.img", image);
    let dst = s.path("out.raw");
    let _ = fs::remove_file(&dst);
    let mut seen = Vec::new();
    let result = convert_to_raw(&src, &dst, |p| seen.push(p));
    let left: Vec<_> = fs::read_dir(&s.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n != "src.img")
        .collect();
    match result {
        Ok(size) => {
            assert_eq!(left, ["out.raw"]);
            assert_eq!(seen.last(), Some(&100));
            assert!(seen.windows(2).all(|w| w[0] < w[1]), "{seen:?}");
            assert_eq!(fs::metadata(&dst).unwrap().len(), size);
            // A corrupted size can describe a huge, empty disk: not read back.
            Ok(if size <= 512 << 20 {
                fs::read(&dst).unwrap()
            } else {
                Vec::new()
            })
        }
        Err(error) => {
            assert!(left.is_empty(), "a refused conversion left {left:?}");
            Err(error)
        }
    }
}

fn refusal(s: &Scratch, image: &[u8]) -> Refusal {
    match convert(s, image) {
        Err(ConvertError::Refused(refusal)) => refusal,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn allocated(path: &Path) -> u64 {
    fs::metadata(path).unwrap().blocks() * 512
}

/// Data, an all-zero data cluster, a zero-flagged allocated cluster with
/// stale bytes, an unallocated L2 range and a partial last cluster.
fn sample_qcow2(version: u32, cluster_bits: u32) -> (Vec<u8>, Vec<u8>) {
    let c = 1u64 << cluster_bits;
    // Three L2 tables' worth where that is small (the middle one unallocated).
    let size = (3 * c * (c / 8)).min(16 * c) - 1536;
    let c = c as usize;
    let last = size.div_ceil(c as u64) - 1;
    let mut clusters = vec![
        (5, Cluster::Data(pattern(c, 1))),
        (1, Cluster::Data(pattern(c, 2))),
        (2, Cluster::Data(pattern(c, 3))),
        (3, Cluster::Data(vec![0; c])),
        (last, Cluster::Data(pattern(c, 4))),
    ];
    if version == 3 {
        clusters.push((4, Cluster::Zero));
        clusters.push((6, Cluster::ZeroAllocated(pattern(c, 5))));
    }
    qcow2(version, cluster_bits, size, &clusters)
}

#[test]
fn qcow2_v2_and_v3_convert_to_the_raw_disk_they_describe() {
    for version in [2, 3] {
        for bits in [9, 16, 21] {
            let s = Scratch::new(&format!("q{version}-{bits}"));
            let (image, raw) = sample_qcow2(version, bits);
            assert!(
                convert(&s, &image).unwrap() == raw,
                "v{version} cluster_bits {bits}"
            );
        }
    }
}

#[test]
fn unallocated_zero_and_all_zero_clusters_stay_holes() {
    let s = Scratch::new("q-holes");
    let c = 64 << 10;
    let size = 256 << 20;
    let (image, raw) = qcow2(
        3,
        16,
        size,
        &[
            (0, Cluster::Data(pattern(c, 9))),
            (100, Cluster::Data(vec![0; c])),
            (200, Cluster::ZeroAllocated(pattern(c, 8))),
            (300, Cluster::Zero),
        ],
    );
    assert!(convert(&s, &image).unwrap() == raw);
    let used = allocated(&s.path("out.raw"));
    assert!(
        used <= 2 * c as u64,
        "{used} bytes allocated for one 64 KiB data cluster"
    );
}

#[test]
fn vhdx_dynamic_and_fixed_convert_to_the_raw_disk_they_describe() {
    let s = Scratch::new("vhdx");
    let size = 6 * MIB as u64 - 512;
    let dynamic = vhdx(
        &VhdxSpec::new(MIB, size),
        &[
            (4, Block::Data(pattern(MIB, 1))),
            (0, Block::Data(pattern(MIB, 2))),
            (1, Block::State(2)),
            (2, Block::State(3)),
            (3, Block::State(1)),
            (5, Block::Data(pattern(MIB, 3))),
        ],
    );
    assert!(convert(&s, &dynamic.0).unwrap() == dynamic.1);

    // A fixed VHDX marks every block present; zero blocks still become holes.
    // (APFS itself fills a hole of 16 MiB or less between written ranges.)
    let blocks: Vec<_> = (0..40)
        .map(|i| {
            (
                i,
                Block::Data(if i % 39 == 0 {
                    pattern(MIB, i)
                } else {
                    vec![0; MIB]
                }),
            )
        })
        .collect();
    let fixed = vhdx(&VhdxSpec::new(MIB, 40 * MIB as u64), &blocks);
    assert!(convert(&s, &fixed.0).unwrap() == fixed.1);
    let used = allocated(&s.path("out.raw"));
    assert!(
        used <= 2 * MIB as u64 + 64 * 1024,
        "{used} bytes allocated for 2 MiB of data"
    );
}

#[test]
fn vhdx_sector_bitmap_entries_are_skipped_in_the_bat() {
    // 512-byte sectors and 1 MiB blocks: chunk ratio 4096, so BAT entry 4096
    // is a sector bitmap and payload block 4096 is entry 4097.
    let s = Scratch::new("vhdx-chunk");
    let size = 4097 * MIB as u64;
    let data = pattern(MIB, 7);
    let (mut image, _) = vhdx(
        &VhdxSpec::new(MIB, size),
        &[(4096, Block::Data(data.clone()))],
    );
    // A present bitmap pointing at the payload must not be read as block 4096.
    let bitmap = VHDX_BAT + 4096 * 8;
    image[bitmap..bitmap + 8].copy_from_slice(&((VHDX_METADATA as u64) | 6).to_le_bytes());
    let src = s.write("src.img", &image);
    let dst = s.path("out.raw");
    assert_eq!(convert_to_raw(&src, &dst, |_| {}).unwrap(), size);
    let out = File::open(&dst).unwrap();
    let mut tail = vec![0u8; MIB];
    out.read_exact_at(&mut tail, 4096 * MIB as u64).unwrap();
    assert!(tail == data);
    assert!(allocated(&dst) <= 2 * MIB as u64);
}

#[test]
fn the_newer_vhdx_header_wins_and_a_damaged_one_is_skipped() {
    let s = Scratch::new("vhdx-headers");
    let (mut image, raw) = vhdx(
        &VhdxSpec::new(MIB, 2 * MIB as u64),
        &[(1, Block::Data(pattern(MIB, 4)))],
    );
    // The newer header (sequence 2) carries a log GUID: it is the one obeyed.
    let newer = VHDX_HEADERS[1];
    image[newer + 48] = 1;
    seal(&mut image[newer..newer + 4096]);
    assert_eq!(refusal(&s, &image), Refusal::VhdxLog);
    // With its checksum broken, the older clean header is used.
    image[newer + 200] ^= 1;
    assert!(convert(&s, &image).unwrap() == raw);
    // Both broken: refused. One broken region table still converts.
    let older = VHDX_HEADERS[0];
    image[older + 200] ^= 1;
    assert!(matches!(refusal(&s, &image), Refusal::Damaged(_)));
    image[older + 200] ^= 1;
    image[VHDX_REGIONS[0] + 100] ^= 1;
    assert!(convert(&s, &image).unwrap() == raw);
}

#[test]
fn refusals_name_what_the_user_must_do() {
    let s = Scratch::new("refusals");
    let (base, _) = sample_qcow2(3, 16);
    let cases: [(usize, u64, usize, Refusal); 7] = [
        (q::BACKING_OFFSET, 4096, 8, Refusal::BackingFile),
        (q::CRYPT, 2, 4, Refusal::Encrypted),
        (q::INCOMPATIBLE, 1, 8, Refusal::Dirty),
        (q::INCOMPATIBLE, 2, 8, Refusal::Corrupt),
        (q::INCOMPATIBLE, 4, 8, Refusal::ExternalDataFile),
        (q::INCOMPATIBLE, 16, 8, Refusal::ExtendedL2),
        (
            q::INCOMPATIBLE,
            1 << 9,
            8,
            Refusal::Unsupported("QCOW2 incompatible features 0x200".into()),
        ),
    ];
    for (at, value, width, expected) in cases {
        let mut image = base.clone();
        patch_be(&mut image, at, value, width);
        assert_eq!(refusal(&s, &image), expected);
    }
    assert!(Refusal::BackingFile
        .to_string()
        .starts_with("이 디스크는 다른 디스크를 기반으로 하니"));

    // A compressed cluster is refused only when one is met.
    let (image, _) = qcow2(
        3,
        16,
        4 << 20,
        &[
            (0, Cluster::Data(pattern(64 << 10, 1))),
            (9, Cluster::Compressed),
        ],
    );
    assert_eq!(refusal(&s, &image), Refusal::Compressed);
    let mut image = base.clone();
    patch_be(&mut image, q::INCOMPATIBLE, 8, 8); // the compression-type bit alone
    assert!(convert(&s, &image).is_ok());

    let size = 2 * MIB as u64;
    let mut parent = VhdxSpec::new(MIB, size);
    parent.has_parent = true;
    assert_eq!(refusal(&s, &vhdx(&parent, &[]).0), Refusal::Differencing);
    let partial = vhdx(&VhdxSpec::new(MIB, size), &[(1, Block::State(7))]);
    assert_eq!(refusal(&s, &partial.0), Refusal::Differencing);
    let mut log = VhdxSpec::new(MIB, size);
    log.log_guid = [7; 16];
    assert_eq!(refusal(&s, &vhdx(&log, &[]).0), Refusal::VhdxLog);
    assert!(Refusal::VhdxLog
        .to_string()
        .contains("Windows에서 이 VHDX를 한 번 연결했다가 분리한 뒤 다시 시도하세요"));
}

#[test]
fn malformed_qcow2_headers_and_tables_fail_cleanly() {
    let s = Scratch::new("q-malformed");
    let (base, _) = sample_qcow2(3, 16);
    let len = base.len() as u64;
    let patched = |at: usize, value: u64, width: usize| {
        let mut image = base.clone();
        patch_be(&mut image, at, value, width);
        image
    };
    for (image, why) in [
        (
            patched(q::L1_OFFSET, len + (64 << 10), 8),
            "L1 past the end",
        ),
        (
            patched(q::L1_OFFSET, !0xffff, 8),
            "L1 offset overflows",
        ),
        (patched(q::L1_OFFSET, 4096, 8), "L1 not cluster aligned"),
        (patched(q::L1_OFFSET, 0, 8), "L1 overlapping the header"),
        (patched(q::L1_SIZE, 0, 4), "L1 smaller than the disk"),
        (patched(q::CLUSTER_BITS, 31, 4), "cluster size"),
        (patched(q::SIZE, 0, 8), "empty disk"),
        (
            patched(q::SIZE, 1 << 50, 8),
            "L1 for a large disk is past the end",
        ),
        (patched(4, 4, 4), "unknown version"),
        (base[..64].to_vec(), "cut-short header"),
    ] {
        assert!(
            matches!(convert(&s, &image), Err(ConvertError::Refused(_))),
            "{why}"
        );
    }
    assert_eq!(
        refusal(&s, &patched(q::SIZE, u64::MAX, 8)),
        Refusal::TooLarge
    );

    // L2 table and data cluster offsets past the end of the file.
    let c = 64 << 10;
    let l1 = c;
    let mut image = base.clone();
    patch_be(&mut image, l1, (len + c as u64) | 1 << 63, 8);
    assert!(matches!(refusal(&s, &image), Refusal::Damaged(_)));
    let mut image = base.clone();
    let l2 = (be64(&image, l1) & 0x00ff_ffff_ffff_fe00) as usize;
    patch_be(&mut image, l2 + 8, (len + (1 << 40)) & !0xffff, 8);
    assert!(matches!(refusal(&s, &image), Refusal::Damaged(_)));
}

#[test]
fn malformed_vhdx_structures_fail_cleanly() {
    let s = Scratch::new("v-malformed");
    let (base, _) = vhdx(
        &VhdxSpec::new(MIB, 3 * MIB as u64),
        &[(0, Block::Data(pattern(MIB, 1)))],
    );
    let len = base.len() as u64;
    let mut cases = Vec::new();
    // The BAT region past the end of the file, in both region tables.
    let mut image = base.clone();
    for at in VHDX_REGIONS {
        image[at + 16 + 16..at + 16 + 24].copy_from_slice(&(len - (MIB as u64 / 2)).to_le_bytes());
        seal(&mut image[at..at + (64 << 10)]);
    }
    cases.push((image, "BAT past the end"));
    // A payload block past the end of the file.
    let mut image = base.clone();
    image[VHDX_BAT..VHDX_BAT + 8].copy_from_slice(&((len + MIB as u64) | 6).to_le_bytes());
    cases.push((image, "block past the end"));
    let mut image = base.clone();
    image[VHDX_BAT..VHDX_BAT + 8].copy_from_slice(&6u64.to_le_bytes());
    cases.push((image, "block inside the header section"));
    // Metadata values: block size, sector size, virtual size, item offset.
    let item = VHDX_METADATA + (64 << 10);
    for (at, value, why) in [
        (item, 3 * MIB as u64, "block size not a power of two"),
        (item + 128, 1000, "sector size"),
        (item + 64, 0, "empty disk"),
        (item + 64, 1 << 60, "too large"),
        (
            VHDX_METADATA + 32 + 16,
            MIB as u64 * 4,
            "item past its region",
        ),
    ] {
        let mut image = base.clone();
        image[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
        if value > u64::from(u32::MAX) {
            image[at..at + 8].copy_from_slice(&value.to_le_bytes());
        }
        cases.push((image, why));
    }
    let mut image = base.clone();
    image[VHDX_METADATA..VHDX_METADATA + 8].copy_from_slice(b"metadat!");
    cases.push((image, "metadata signature"));
    cases.push((base[..MIB / 2].to_vec(), "cut-short header section"));
    for (image, why) in cases {
        assert!(
            matches!(convert(&s, &image), Err(ConvertError::Refused(_))),
            "{why}"
        );
    }
}

#[test]
fn corrupted_and_truncated_images_never_panic_or_leave_output() {
    let s = Scratch::new("fuzz");
    let images = [sample_qcow2(3, 9).0, sample_qcow2(2, 16).0, {
        vhdx(
            &VhdxSpec::new(MIB, 3 * MIB as u64),
            &[(1, Block::Data(pattern(MIB, 2)))],
        )
        .0
    }];
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    for image in &images {
        for cut in (0..image.len()).step_by(image.len() / 30 + 1) {
            let _ = convert(&s, &image[..cut]);
        }
        for _ in 0..150 {
            let mut bad = image.clone();
            for _ in 0..4 {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                // Mostly the metadata at the front, where damage matters.
                let at = (x as usize >> 8) % bad.len().min(if x & 1 == 0 { 4096 } else { 3 * MIB });
                bad[at] = (x >> 56) as u8;
            }
            let _ = convert(&s, &bad);
        }
    }
}

#[test]
fn an_existing_destination_is_never_replaced() {
    let s = Scratch::new("exists");
    let (image, _) = sample_qcow2(3, 16);
    let src = s.write("src.img", &image);
    let dst = s.write("out.raw", b"keep");
    assert!(matches!(
        convert_to_raw(&src, &dst, |_| {}),
        Err(ConvertError::Io(e)) if e.kind() == io::ErrorKind::AlreadyExists
    ));
    assert_eq!(fs::read(&dst).unwrap(), b"keep");
    let link = s.path("link.raw");
    std::os::unix::fs::symlink(s.path("elsewhere"), &link).unwrap();
    assert!(convert_to_raw(&src, &link, |_| {}).is_err());
    assert!(!s.path("elsewhere").exists());
}

#[test]
fn a_shortfall_of_free_space_is_one_sentence_with_the_numbers() {
    let text = Refusal::NoSpace {
        needed: 40_000_000_000,
        available: 12_500_000_000,
    }
    .to_string();
    assert_eq!(text, "변환한 디스크를 저장할 공간이 부족하니 대상 볼륨에 28 GB 이상을 비운 뒤 다시 시도하세요(필요 40 GB, 남은 공간 12 GB).");
}

#[test]
fn the_cli_reports_progress_size_and_refusals() {
    let s = Scratch::new("cli");
    let (image, raw) = sample_qcow2(3, 16);
    let src = s.write("src.img", &image);
    let dst = s.path("out.raw");
    let args = [src.display().to_string(), dst.display().to_string()];
    assert_eq!(command(&args), ExitCode::SUCCESS);
    assert!(fs::read(&dst).unwrap() == raw);
    assert_eq!(command(&args), ExitCode::FAILURE);
    assert_eq!(command(&args[..1]), ExitCode::from(2));
}
