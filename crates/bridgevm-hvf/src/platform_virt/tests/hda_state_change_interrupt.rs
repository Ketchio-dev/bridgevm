//! HDA codec State Change interrupts use matching WAKEEN, CIE, and GIE enables.

use super::*;

fn codec_detected() -> Fixture {
    let mut f = Fixture::new();
    f.write(REG_GCTL, 4, 1);
    assert_eq!(f.read(REG_STATESTS, 2), 1);
    assert!(f.messages().is_empty());
    f
}

#[test]
fn masked_codec_state_change_does_not_deliver_msi() {
    let mut f = codec_detected();
    f.write(REG_INTCTL, 4, GIE | CIE);
    for wakeen in [0, 2, 0x7ffe] {
        f.write(REG_WAKEEN, 2, wakeen);
        assert_eq!(f.read(REG_STATESTS, 2), 1);
        assert!(!f.level());
        assert!(f.messages().is_empty());
    }
}

#[test]
fn matching_codec_wake_enable_delivers_one_msi_and_rearms_after_mask() {
    let mut f = codec_detected();
    f.write(REG_INTCTL, 4, GIE | CIE);
    f.write(REG_WAKEEN, 1, 1);
    assert!(f.level());
    assert_eq!(f.read(REG_INTSTS, 4), GIE | CIE);
    f.expect_message();
    f.write(REG_WAKEEN, 1, 1);
    assert!(f.messages().is_empty());
    f.write(REG_WAKEEN, 1, 0);
    assert!(!f.level());
    assert_eq!(f.read(REG_STATESTS, 2), 1);
    assert!(f.messages().is_empty());
    f.write(REG_WAKEEN, 1, 1);
    assert!(f.level());
    f.expect_message();
}

#[test]
fn codec_state_change_respects_controller_and_global_masks() {
    for intctl in [0, CIE, GIE] {
        let mut f = codec_detected();
        f.write(REG_WAKEEN, 2, 1);
        f.write(REG_INTCTL, 4, intctl);
        assert!(!f.level());
        assert!(f.messages().is_empty());
        f.write(REG_INTCTL, 4, GIE | CIE);
        assert!(f.level());
        f.expect_message();
        f.write(REG_INTCTL, 4, intctl);
        assert!(!f.level());
        assert_eq!(f.read(REG_STATESTS, 2), 1);
        assert!(f.messages().is_empty());
    }
}

#[test]
fn state_change_write_one_to_clear_deasserts_without_replaying_msi() {
    let mut f = codec_detected();
    f.write(REG_INTCTL, 4, GIE | CIE);
    f.write(REG_WAKEEN, 2, 1);
    f.expect_message();
    for (offset, size, value) in [
        (REG_STATESTS, 2, 0),
        (REG_STATESTS, 2, 2),
        (REG_STATESTS + 1, 1, 0xff),
    ] {
        f.write(offset, size, value);
        assert_eq!(f.read(REG_STATESTS, 2), 1);
        assert!(f.level());
        assert!(f.messages().is_empty());
    }
    f.write(REG_STATESTS, 1, 1);
    assert_eq!(f.read(REG_STATESTS, 2), 0);
    assert!(!f.level());
    assert_eq!(f.read(REG_INTSTS, 4), 0);
    assert!(f.messages().is_empty());
    f.write(REG_WAKEEN, 2, 0);
    f.write(REG_WAKEEN, 2, 1);
    assert!(!f.level());
    assert!(f.messages().is_empty());
}
