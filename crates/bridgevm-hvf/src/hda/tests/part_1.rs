//! Split test module.

use super::super::*;
use crate::platform_virt::FlatGuestRam;
use crate::{fwcfg::GuestMemoryMut, msix::MsixMessage};
use std::fs;
use std::{path::Path, time::Duration};

use super::helpers::*;

#[test]
fn controller_reset_flow_and_register_semantics() {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    let mut mem = FlatGuestRam::new(RAM_BASE, 0x10000);
    assert_eq!(ctrl.mmio_read(REG_GCAP, 2), u64::from(GCAP_64OK_ONE_OUTPUT));
    assert_eq!(ctrl.mmio_read(0x02, 2), 0x0100);
    assert_eq!(ctrl.mmio_read(REG_GCTL, 4), 0);
    assert_eq!(ctrl.mmio_read(REG_CORBSIZE, 1), 0xe2);
    assert_eq!(ctrl.mmio_read(REG_RIRBSIZE, 1), 0xe2);

    write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
    assert_eq!(ctrl.mmio_read(REG_GCTL, 4) & 1, 1);
    assert_eq!(ctrl.mmio_read(REG_STATESTS, 2), 1);
    write(&mut ctrl, &mut mem, REG_STATESTS, 2, 1);
    assert_eq!(ctrl.mmio_read(REG_STATESTS, 2), 0);

    write(&mut ctrl, &mut mem, REG_SD_CTL, 1, SDCTL_SRST as u64);
    assert_eq!(ctrl.mmio_read(REG_SD_CTL, 1), 1);
    write(&mut ctrl, &mut mem, REG_SD_CTL, 1, 0);
    assert_eq!(ctrl.mmio_read(REG_SD_CTL, 1), 0);
    write(&mut ctrl, &mut mem, REG_CORBRP, 2, 0x8000);
    assert_eq!(ctrl.mmio_read(REG_CORBRP, 2), 0x8000);
    write(&mut ctrl, &mut mem, REG_CORBRP, 2, 0);
    assert_eq!(ctrl.mmio_read(REG_CORBRP, 2), 0);
    write(&mut ctrl, &mut mem, REG_CORBSIZE, 1, 0);
    write(&mut ctrl, &mut mem, REG_RIRBSIZE, 1, 1);
    assert_eq!(ctrl.mmio_read(REG_CORBSIZE, 1), 0xe0);
    assert_eq!(ctrl.mmio_read(REG_RIRBSIZE, 1), 0xe1);
    write(&mut ctrl, &mut mem, REG_GCTL, 1, 0);
    assert_eq!(ctrl.mmio_read(REG_GCTL, 4), 0);
}

#[test]
fn codec_widget_graph_exposes_fixed_speaker_output_path() {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);

    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_ROOT, 0xf00, 0x00)),
        CODEC_VENDOR_ID
    );
    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_ROOT, 0xf00, 0x02)),
        CODEC_REVISION_ID
    );
    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_ROOT, 0xf00, 0x04)),
        0x0001_0001
    );

    assert_eq!(ctrl.codec_verb(verb(0, CODEC_AFG, 0xf00, 0x05)), 1);
    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_AFG, 0xf00, 0x04)),
        CODEC_AFG_CHILD_NODE_COUNT
    );
    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_AFG, 0xf00, 0x08)),
        CODEC_AFG_CAPABILITIES
    );
    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_AFG, 0xf00, 0x0a)),
        CODEC_PCM_SIZE_RATES
    );
    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_AFG, 0xf00, 0x0b)),
        CODEC_STREAM_FORMATS
    );

    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_DAC, 0xf00, 0x09)),
        DAC_WIDGET_CAPABILITIES
    );
    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_DAC, 0xf00, 0x12)),
        CODEC_OUTPUT_AMP_CAPS
    );

    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_SPEAKER, 0xf00, 0x09)),
        SPEAKER_WIDGET_CAPABILITIES
    );
    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_SPEAKER, 0xf00, 0x0c)),
        SPEAKER_PIN_CAPABILITIES
    );
    assert_eq!(ctrl.codec_verb(verb(0, CODEC_SPEAKER, 0xf00, 0x0e)), 1);
    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_SPEAKER, 0xf02, 0)),
        u32::from(CODEC_DAC)
    );
    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_SPEAKER, 0xf1c, 0)),
        SPEAKER_CONFIG_DEFAULT
    );
    assert_eq!(ctrl.codec_verb(verb(0, CODEC_SPEAKER, 0xf07, 0)), 0x40);
    assert_eq!(
        ctrl.codec_verb(verb(0, CODEC_SPEAKER, 0xf09, 0)),
        SPEAKER_PIN_SENSE
    );
}

#[test]
fn codec_afg_exposes_children_and_all_enumeration_parameters() {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    let child_count = ctrl.codec_verb(0x001f_0004);
    let first_child = ((child_count >> 16) & 0xff) as u8;
    let children = (child_count & 0xff) as u8;

    // 0x001f0004 is CAD 0, NID 1, GET_PARAMETER (0xf00), parameter 0x04.
    assert_eq!(child_count, CODEC_AFG_CHILD_NODE_COUNT);
    assert_eq!(first_child, CODEC_DAC);
    assert_eq!(children, CODEC_SPEAKER - CODEC_DAC + 1);
    assert_eq!(first_child + children - 1, CODEC_SPEAKER);
    for parameter in [0x10, 0x13] {
        assert_eq!(afg_parameter(parameter), None);
        assert_eq!(ctrl.codec_verb(verb(0, CODEC_AFG, 0xf00, parameter)), 0);
    }

    let expected = [
        (0x01, CODEC_IMPLEMENTATION_ID), // AC_PAR_SUBSYSTEM_ID
        (0x04, CODEC_AFG_CHILD_NODE_COUNT),
        (0x05, 0x0000_0001),
        (0x08, CODEC_AFG_CAPABILITIES),
        (0x0a, CODEC_PCM_SIZE_RATES),
        (0x0b, CODEC_STREAM_FORMATS),
        (0x0d, 0),
        (0x0f, CODEC_AFG_POWER_STATES),
        (0x11, 0),
        (0x12, 0),
    ];
    for (parameter, value) in expected {
        assert_eq!(
            afg_parameter(parameter),
            Some(value),
            "AFG parameter {parameter:#04x} must be explicitly handled"
        );
        assert_eq!(
            ctrl.codec_verb(verb(0, CODEC_AFG, 0xf00, parameter)),
            value,
            "AFG GET_PARAMETER {parameter:#04x}"
        );
    }
}

#[test]
fn codec_enumeration_reports_subsystem_id_on_root_and_afg() {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    // GET_PARAMETER(0x01) is AC_PAR_SUBSYSTEM_ID (intel-hda-defs.h), NOT a
    // reserved parameter: hdaudio.sys queries it during enumeration and
    // rejects a codec whose function group reports 0, so both the root and
    // the AFG must return a valid subsystem id (matching QEMU's output
    // codec, which exposes AC_PAR_SUBSYSTEM_ID on both nodes).
    let observed = [
        (0x000f_0000, CODEC_VENDOR_ID),         // AC_PAR_VENDOR_ID
        (0x000f_0001, CODEC_IMPLEMENTATION_ID), // root AC_PAR_SUBSYSTEM_ID
        (0x000f_0002, CODEC_REVISION_ID),       // AC_PAR_REV_ID
        (0x000f_0004, 0x0001_0001),             // AC_PAR_NODE_COUNT
        (0x001f_0001, CODEC_IMPLEMENTATION_ID), // AFG AC_PAR_SUBSYSTEM_ID
        (0x001f_0005, 0x0000_0001),             // AC_PAR_FUNCTION_TYPE=audio
        (0x001f_0500, 0),                       // AFG GET_POWER_STATE (D0)
    ];

    for (command, response) in observed {
        assert_eq!(
            ctrl.codec_verb(command),
            response,
            "command {command:#010x}"
        );
    }

    // The GET_SUBSYSTEM_ID verb (F20) returns the same id as the parameter.
    assert_eq!(ctrl.codec_verb(0x001f_2000), CODEC_IMPLEMENTATION_ID);
    assert_eq!(ctrl.codec_verb(0x000f_2000), 0);
}

#[test]
fn codec_output_widget_get_set_verbs_round_trip() {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);

    assert_eq!(ctrl.codec_verb(verb(0, CODEC_DAC, 0x706, 0x21)), 0);
    assert_eq!(ctrl.codec_verb(verb(0, CODEC_DAC, 0xf06, 0)), 0x21);
    assert_eq!(ctrl.codec_verb(verb16(0, CODEC_DAC, 0x2, 0x4011)), 0);
    assert_eq!(ctrl.codec_verb(verb16(0, CODEC_DAC, 0xa, 0)), 0x4011);

    assert_eq!(ctrl.codec_verb(verb(0, CODEC_SPEAKER, 0x707, 0)), 0);
    assert_eq!(ctrl.codec_verb(verb(0, CODEC_SPEAKER, 0xf07, 0)), 0);
    assert_eq!(ctrl.codec_verb(verb(0, CODEC_SPEAKER, 0x707, 0x40)), 0);
    assert_eq!(ctrl.codec_verb(verb(0, CODEC_SPEAKER, 0xf07, 0)), 0x40);

    assert_eq!(ctrl.codec_verb(verb(0, CODEC_DAC, 0x705, 3)), 0);
    assert_eq!(ctrl.codec_verb(verb(0, CODEC_DAC, 0xf05, 0)), 0x33);
    assert_eq!(ctrl.codec_verb(verb(0, CODEC_SPEAKER, 0xf05, 0)), 0);
}

#[test]
fn corb_rirb_enumeration_verb_round_trip() {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    let mut mem = FlatGuestRam::new(RAM_BASE, 0x10000);
    let corb = RAM_BASE + 0x1000;
    let rirb = RAM_BASE + 0x2000;
    let commands = [
        verb(0, CODEC_ROOT, 0xf00, 0x00),
        verb(0, CODEC_ROOT, 0xf00, 0x04),
        verb(0, CODEC_AFG, 0xf00, 0x04),
        verb(0, CODEC_SPEAKER, 0xf00, 0x09),
        verb(0, CODEC_SPEAKER, 0xf1c, 0),
    ];
    for (index, command) in commands.iter().enumerate() {
        assert!(mem.write_bytes(corb + (index as u64 + 1) * 4, &command.to_le_bytes()));
    }

    write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
    write(&mut ctrl, &mut mem, REG_CORBLBASE, 4, corb);
    write(&mut ctrl, &mut mem, REG_RIRBLBASE, 4, rirb);
    write(&mut ctrl, &mut mem, REG_RINTCNT, 2, commands.len() as u64);
    write(
        &mut ctrl,
        &mut mem,
        REG_RIRBCTL,
        1,
        u64::from(RIRBCTL_DMA | RIRBCTL_RINTCTL),
    );
    write(&mut ctrl, &mut mem, REG_CORBCTL, 1, u64::from(CORBCTL_RUN));
    write(&mut ctrl, &mut mem, REG_CORBWP, 2, commands.len() as u64);

    let responses: Vec<u32> = (1..=commands.len())
        .map(|index| {
            let bytes = mem.read_bytes(rirb + index as u64 * 8, 4).unwrap();
            u32::from_le_bytes(bytes.try_into().unwrap())
        })
        .collect();
    assert_eq!(
        responses,
        vec![
            CODEC_VENDOR_ID,
            0x0001_0001,
            0x0002_0002,
            SPEAKER_WIDGET_CAPABILITIES,
            SPEAKER_CONFIG_DEFAULT
        ]
    );
    assert_eq!(ctrl.mmio_read(REG_CORBRP, 2), commands.len() as u64);
    assert_eq!(ctrl.mmio_read(REG_RIRBWP, 2), commands.len() as u64);
    assert_ne!(
        ctrl.mmio_read(REG_RIRBSTS, 1) & u64::from(RIRBSTS_RINTFL),
        0
    );
}

#[test]
fn stream_bdl_dma_captures_pcm_updates_position_and_raises_ioc() {
    let output = temp_path("pcm.raw");
    fs::remove_file(&output).ok();
    let mut ctrl = HdaController::with_pcm_output_path(Some(&output));
    let mut mem = FlatGuestRam::new(RAM_BASE, 0x10000);
    let bdl = RAM_BASE + 0x1000;
    let pcm = RAM_BASE + 0x2000;
    let dp = RAM_BASE + 0x3000;
    let expected: Vec<u8> = (0..192).map(|value| value as u8).collect();
    assert!(mem.write_bytes(pcm, &expected));
    let mut descriptor = [0u8; 16];
    descriptor[..8].copy_from_slice(&pcm.to_le_bytes());
    descriptor[8..12].copy_from_slice(&(expected.len() as u32).to_le_bytes());
    descriptor[12..16].copy_from_slice(&BDL_IOC.to_le_bytes());
    assert!(mem.write_bytes(bdl, &descriptor));

    write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
    write(&mut ctrl, &mut mem, REG_DPLBASE, 4, dp | 1);
    write(&mut ctrl, &mut mem, REG_SD_BDPL, 4, bdl);
    write(&mut ctrl, &mut mem, REG_SD_CBL, 4, expected.len() as u64);
    write(&mut ctrl, &mut mem, REG_SD_LVI, 2, 0);
    write(&mut ctrl, &mut mem, REG_SD_FMT, 2, 0x0011); // 48 kHz, s16, stereo.
    write(
        &mut ctrl,
        &mut mem,
        REG_INTCTL,
        4,
        u64::from(INTCTL_GIE | INTCTL_STREAM0),
    );
    write(
        &mut ctrl,
        &mut mem,
        REG_SD_CTL,
        1,
        u64::from(SDCTL_RUN | SDCTL_IOCE),
    );
    ctrl.poll_for_duration(&mut mem, Duration::from_millis(1));

    assert_eq!(
        ctrl.mmio_read(REG_SD_LPIB, 4),
        0,
        "CBL wraps LPIB after one full buffer"
    );
    assert_ne!(ctrl.mmio_read(REG_SD_STS, 1) & u64::from(SDSTS_BCIS), 0);
    assert!(ctrl.interrupt_level());
    assert_eq!(
        u32::from_le_bytes(mem.read_bytes(dp, 4).unwrap().try_into().unwrap()),
        0
    );
    drop(ctrl);
    assert_eq!(fs::read(&output).unwrap(), expected);
    fs::remove_file(output).ok();
}

#[test]
fn zero_length_bdl_entry_stops_stream_with_descriptor_error() {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    let mut mem = FlatGuestRam::new(RAM_BASE, 0x10000);
    let bdl = RAM_BASE + 0x1000;
    assert!(mem.write_bytes(bdl, &[0; 16]));

    write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
    write(&mut ctrl, &mut mem, REG_SD_BDPL, 4, bdl);
    write(&mut ctrl, &mut mem, REG_SD_CBL, 4, 192);
    write(&mut ctrl, &mut mem, REG_SD_LVI, 2, 0);
    write(&mut ctrl, &mut mem, REG_SD_FMT, 2, 0x0011);
    write(&mut ctrl, &mut mem, REG_SD_CTL, 1, u64::from(SDCTL_RUN));
    ctrl.poll_for_duration(&mut mem, Duration::from_millis(1));

    assert_eq!(ctrl.mmio_read(REG_SD_CTL, 1) & u64::from(SDCTL_RUN), 0);
    assert_ne!(ctrl.mmio_read(REG_SD_STS, 1) & u64::from(SDSTS_DESE), 0);
}

#[test]
fn immediate_codec_command_reports_speaker_topology() {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    let mut mem = FlatGuestRam::new(RAM_BASE, 0x1000);
    write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
    write(
        &mut ctrl,
        &mut mem,
        REG_ICOI,
        4,
        u64::from(verb(0, CODEC_SPEAKER, 0xf1c, 0)),
    );
    write(&mut ctrl, &mut mem, REG_ICIS, 2, u64::from(ICIS_ICB));
    assert_eq!(
        ctrl.mmio_read(REG_ICII, 4),
        u64::from(SPEAKER_CONFIG_DEFAULT)
    );
    assert_eq!(ctrl.mmio_read(REG_ICIS, 2), u64::from(ICIS_IRV));
    write(&mut ctrl, &mut mem, REG_ICIS, 2, u64::from(ICIS_IRV));
    assert_eq!(ctrl.mmio_read(REG_ICIS, 2), 0);
}

#[test]
fn controller_and_stream_sources_raise_one_programmed_hda_msi() {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    let mut mem = FlatGuestRam::new(RAM_BASE, 0x1000);
    let message_address = 0x0000_0001_0808_2000;

    ctrl.rirb_sts = RIRBSTS_RINTFL;
    ctrl.rirb_ctl = RIRBCTL_RINTCTL;
    ctrl.intctl = INTCTL_GIE | INTCTL_CIE;
    let mut messages = Vec::new();
    ctrl.drain_pending_msi_into(false, message_address, 0x41, &mut messages);
    assert!(messages.is_empty(), "disabled MSI must remain pending");

    ctrl.drain_pending_msi_into(true, message_address, 0x41, &mut messages);
    assert_eq!(
        messages,
        vec![MsixMessage {
            vector: MSI_CONTROLLER_VECTOR,
            address: message_address,
            data: 0x41,
        }]
    );

    write(
        &mut ctrl,
        &mut mem,
        REG_RIRBSTS,
        1,
        u64::from(RIRBSTS_RINTFL),
    );
    messages.clear();
    ctrl.drain_pending_msi_into(true, message_address, 0x41, &mut messages);
    assert!(messages.is_empty());

    ctrl.stream.sts = SDSTS_BCIS;
    ctrl.stream.ctl = SDCTL_IOCE;
    ctrl.intctl = INTCTL_GIE | INTCTL_STREAM0;
    ctrl.drain_pending_msi_into(true, message_address, 0x41, &mut messages);
    assert_eq!(
        messages,
        vec![MsixMessage {
            vector: MSI_CONTROLLER_VECTOR,
            address: message_address,
            data: 0x41,
        }]
    );
}
