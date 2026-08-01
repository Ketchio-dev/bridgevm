//! EDID base-block construction: the byte format the guest driver parses to
//! decide which display modes exist.

pub(crate) fn build_edid(width: u32, height: u32) -> [u8; 128] {
    let mut edid = [0u8; 128];
    edid[0..8].copy_from_slice(&[0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00]);
    edid[8..10].copy_from_slice(&encode_manufacturer("BVM"));
    edid[10..12].copy_from_slice(&0x0001u16.to_le_bytes());
    edid[12..16].copy_from_slice(&1u32.to_le_bytes());
    edid[16] = 1;
    edid[17] = 34;
    edid[18] = 1;
    edid[19] = 4;
    edid[20] = 0xa5;
    edid[21] = ((width / 100).clamp(1, 255)) as u8;
    edid[22] = ((height / 100).clamp(1, 255)) as u8;
    edid[23] = 0x78;
    edid[24] = 0x0a;
    edid[25] = 0xcf;
    edid[26] = 0x74;
    edid[27] = 0xa3;
    edid[28] = 0x57;
    edid[29] = 0x4c;
    edid[30] = 0xb0;
    edid[31] = 0x23;
    edid[32] = 0x09;
    // Established timings I/II. viogpu3d builds its entire mode list from these
    // bits plus the standard timings below (VioGpuVidPN::AddEdidModes), so an
    // EDID that advertises almost nothing leaves the guest with almost no modes
    // to pick from -- which is what stopped dynamic resize.
    edid[35] = 0xef; // 720x400@70, 640x480 @60/72/75, 800x600 @56/60
    edid[36] = 0xef; // 800x600 @72/75, 832x624@75, 1024x768 @60/70/75, 1280x1024@75
    edid[37] = 0x80; // manufacturer: 1152x870@75

    // Standard timings (8 two-byte slots). Each is
    // ((h_active / 8) - 31, aspect << 6 | (refresh - 60)).
    // The horizontal field is (h_active / 8) - 31 in a single byte, so anything
    // above 2288 wide cannot be expressed here; those rely on the DTD instead.
    let standard = [
        (1280, 720),
        (1280, 800),
        (1280, 960),
        (1440, 900),
        (1600, 900),
        (1680, 1050),
        (1920, 1080),
        (2048, 1152),
    ];
    for (slot, (w, h)) in standard.iter().enumerate() {
        let bytes = standard_timing(*w, *h);
        edid[38 + slot * 2] = bytes[0];
        edid[39 + slot * 2] = bytes[1];
    }

    let dtd = detailed_timing_descriptor(width, height, 120);
    let pixel_clock_10khz = u16::from_le_bytes([dtd[0], dtd[1]]);
    let max_pixel_clock_10mhz = pixel_clock_10khz.div_ceil(1_000) as u8;
    edid[54..72].copy_from_slice(&dtd);
    edid[72..90].copy_from_slice(&monitor_descriptor(
        0xfd,
        &[
            48,
            144,
            30,
            160,
            max_pixel_clock_10mhz,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ],
    ));
    edid[90..108].copy_from_slice(&monitor_descriptor_text(0xfc, b"BridgeVM GPU"));
    edid[108..126].copy_from_slice(&monitor_descriptor_text(0xfe, b"virtio-gpu"));
    edid[126] = 0;
    let sum = edid[..127]
        .iter()
        .fold(0u8, |acc, byte| acc.wrapping_add(*byte));
    edid[127] = 0u8.wrapping_sub(sum);
    edid
}

/// One EDID standard-timing slot at 60 Hz, or the unused marker when the mode
/// cannot be encoded (the horizontal field only spans 256..2288 in steps of 8).
fn standard_timing(width: u32, height: u32) -> [u8; 2] {
    let Some(encoded_width) = (width / 8).checked_sub(31) else {
        return [0x01, 0x01];
    };
    if width % 8 != 0 || encoded_width == 0 || encoded_width > 255 || height == 0 {
        return [0x01, 0x01];
    }
    // Aspect is stored, not the height, so only the four ratios EDID knows fit.
    let aspect = match (width * 1000) / height {
        1600 => 0b00, // 16:10
        1333 => 0b01, // 4:3
        1250 => 0b10, // 5:4
        1777 => 0b11, // 16:9
        _ => return [0x01, 0x01],
    };
    // Low six bits are (refresh - 60), so a plain aspect field means 60 Hz.
    [encoded_width as u8, aspect << 6]
}

pub(crate) fn detailed_timing_descriptor(width: u32, height: u32, refresh_hz: u32) -> [u8; 18] {
    let h_blank = 160u32.max(width / 8);
    let v_blank = 45u32.max(height / 20);
    let h_sync_offset = 48u32.min(h_blank / 3);
    let h_sync_width = 32u32.min(h_blank.saturating_sub(h_sync_offset).max(1));
    let v_sync_offset = 3u32;
    let v_sync_width = 5u32;
    let requested_pixel_clock_10khz = ((u64::from(width) + u64::from(h_blank))
        * (u64::from(height) + u64::from(v_blank))
        * u64::from(refresh_hz)
        / 10_000)
        .max(1);
    let pixel_clock_10khz = requested_pixel_clock_10khz.min(u64::from(u16::MAX));
    if requested_pixel_clock_10khz > u64::from(u16::MAX) {
        eprintln!(
            "virtio-gpu EDID: {width}x{height}@{refresh_hz} requires pixel clock \
             {requested_pixel_clock_10khz}0 kHz; clamping to {}0 kHz",
            u16::MAX
        );
    }

    let mut dtd = [0u8; 18];
    dtd[0..2].copy_from_slice(&(pixel_clock_10khz as u16).to_le_bytes());
    dtd[2] = width as u8;
    dtd[3] = h_blank as u8;
    dtd[4] = (((width >> 8) as u8) << 4) | ((h_blank >> 8) as u8 & 0x0f);
    dtd[5] = height as u8;
    dtd[6] = v_blank as u8;
    dtd[7] = (((height >> 8) as u8) << 4) | ((v_blank >> 8) as u8 & 0x0f);
    dtd[8] = h_sync_offset as u8;
    dtd[9] = h_sync_width as u8;
    dtd[10] = ((v_sync_offset as u8) << 4) | (v_sync_width as u8 & 0x0f);
    dtd[11] = (((h_sync_offset >> 8) as u8 & 0x03) << 6)
        | (((h_sync_width >> 8) as u8 & 0x03) << 4)
        | (((v_sync_offset >> 4) as u8 & 0x03) << 2)
        | ((v_sync_width >> 4) as u8 & 0x03);
    dtd[12] = ((width * 254 / 96) / 10).min(4095) as u8;
    dtd[13] = ((height * 254 / 96) / 10).min(4095) as u8;
    dtd[14] = ((((width * 254 / 96) / 10) >> 8) as u8 & 0x0f) << 4
        | ((((height * 254 / 96) / 10) >> 8) as u8 & 0x0f);
    dtd[17] = 0x1a;
    dtd
}

pub(crate) fn monitor_descriptor(tag: u8, payload: &[u8]) -> [u8; 18] {
    let mut desc = [0u8; 18];
    desc[3] = tag;
    let n = payload.len().min(13);
    desc[5..5 + n].copy_from_slice(&payload[..n]);
    desc
}

pub(crate) fn monitor_descriptor_text(tag: u8, text: &[u8]) -> [u8; 18] {
    let mut payload = [b' '; 13];
    let n = text.len().min(12);
    payload[..n].copy_from_slice(&text[..n]);
    payload[n] = b'\n';
    monitor_descriptor(tag, &payload)
}

pub(crate) fn encode_manufacturer(value: &str) -> [u8; 2] {
    let mut code = 0u16;
    for byte in value.bytes().take(3) {
        let letter = u16::from(byte.to_ascii_uppercase().saturating_sub(b'@') & 0x1f);
        code = (code << 5) | letter;
    }
    code.to_be_bytes()
}
