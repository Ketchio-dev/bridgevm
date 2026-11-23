//! EDID mode-advertisement coverage: the guest builds its entire display mode
//! list from these bytes, so their content decides whether resize is possible.

use crate::virtio_gpu::display::edid::build_edid;

#[test]
fn edid_advertises_enough_modes_for_the_guest_to_resize_into() {
    // viogpu3d builds its whole mode list from the established-timing bits and
    // the standard-timing slots (VioGpuVidPN::AddEdidModes). With only 720x400
    // and 800x600 advertised, the guest enumerated a single mode and had nothing
    // to switch to, which is what blocked dynamic resize.
    let edid = build_edid(1280, 800);

    assert_eq!(edid[35], 0xef);
    assert_eq!(edid[36], 0xef);
    assert_eq!(edid[37], 0x80);

    let unused = [0x01, 0x01];
    let slots: Vec<[u8; 2]> = (0..8)
        .map(|slot| [edid[38 + slot * 2], edid[39 + slot * 2]])
        .collect();
    assert!(
        slots.iter().all(|slot| *slot != unused),
        "every standard-timing slot should carry a mode: {slots:?}"
    );

    // 1600x900 is the geometry the resize gate asks for, so it must be encodable:
    // (1600 / 8) - 31 = 169, aspect 16:9.
    assert!(slots.contains(&[169, 0b11 << 6]), "{slots:?}");

    assert_eq!(
        edid[..128]
            .iter()
            .fold(0u8, |sum, byte| sum.wrapping_add(*byte)),
        0
    );
}

#[test]
fn edid_lists_3840x2160_where_viogpudo_reads_it_at_driver_start() {
    // viogpudo sizes its framebuffer memory for the largest mode it lists at
    // driver start (16 MiB at least) and refuses larger modes later; live,
    // 3456x2234 (30.9 MB) failed with ERROR_NOT_ENOUGH_MEMORY against 16 MiB.
    // A 3840x2160 VIC makes that 33,177,600 bytes.
    let edid = build_edid(1280, 800);
    assert_eq!(edid[126], 1, "one extension block");
    let ext = &edid[128..];
    assert_eq!((ext[0], ext[1]), (0x02, 0x03), "CTA-861 revision 3");
    assert_eq!(ext.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)), 0);

    // As the standard reads it: one video data block, then no detailed timings.
    let (tag, len) = (ext[4] >> 5, usize::from(ext[4] & 0x1f));
    assert_eq!((tag, len), (2, 2));
    assert_eq!(usize::from(ext[2]), 5 + len);
    assert_eq!(&ext[5..5 + len], &[97, 16]);
    assert!(ext[usize::from(ext[2])..127].iter().all(|b| *b == 0));

    // As viogpudo's AddEdidModes reads it: (byte 2 - 5) VIC numbers from byte
    // 4, each looked up as table[vic - 1] if below 128. VIC 97 (3840x2160) is
    // among them, and none is 0, which it would look up out of bounds.
    let vics = &ext[4..4 + usize::from(ext[2]) - 5];
    assert!(vics.contains(&97), "{vics:?}");
    assert!(vics.iter().all(|v| (1..128).contains(v)), "{vics:?}");
}
