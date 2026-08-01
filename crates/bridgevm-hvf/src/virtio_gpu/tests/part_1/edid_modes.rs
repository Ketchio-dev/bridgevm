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
        edid.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)),
        0
    );
}
