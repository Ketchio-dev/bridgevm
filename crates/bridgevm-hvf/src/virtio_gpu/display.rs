//! Display topology: GET_DISPLAY_INFO, host-driven resolution change, EDID generation.

use super::*;

/// virtio-gpu config `events_read` bit: the host changed the scanout layout
/// (resolution), so the guest should re-query GET_DISPLAY_INFO/GET_EDID.
pub(crate) const VIRTIO_GPU_EVENT_DISPLAY: u32 = 1 << 0;

/// Largest scanout the resize path accepts, matching the EDID/mode range the
/// viogpu3d driver advertises. Guards the scanout allocation.
pub(crate) const MAX_SCANOUT_DIMENSION: u32 = 7680;

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
    edid[35] = 0x81;
    edid[36] = 0x80;

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

impl VirtioGpu {
    /// Host-driven scanout resize. Updates the reported resolution and raises a
    /// virtio-gpu DISPLAY event + config-change interrupt so the guest WDDM
    /// driver re-queries GET_DISPLAY_INFO/GET_EDID and switches modes. No-op
    /// (returns false) when the size is unchanged or out of range; the caller
    /// delivers the config interrupt via the device wrapper's drain path.
    pub(crate) fn request_display_resolution(&mut self, width: u32, height: u32) -> bool {
        if width == 0
            || height == 0
            || width > MAX_SCANOUT_DIMENSION
            || height > MAX_SCANOUT_DIMENSION
        {
            return false;
        }
        if width == self.width && height == self.height {
            return false;
        }
        self.width = width;
        self.height = height;
        // Grow the 2D scanout backing to the new geometry; the guest re-creates
        // its scanout resource after the mode switch, so drop the stale binding.
        self.scanout.clear();
        self.scanout.resize(scanout_len(width, height), 0);
        self.scanout_resource = None;
        self.unbind_blob_scanout();
        self.events_read |= VIRTIO_GPU_EVENT_DISPLAY;
        self.pending_config_change = true;
        self.interrupt_status |= 2;
        true
    }

    pub(crate) fn response_display_info_into(&self, hdr: Option<CtrlHdr>, out: &mut Vec<u8>) {
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_DISPLAY_INFO, hdr);
        for scanout in 0..16 {
            if scanout == 0 {
                push_rect(
                    out,
                    Rect {
                        x: 0,
                        y: 0,
                        width: self.width,
                        height: self.height,
                    },
                );
                out.extend_from_slice(&1u32.to_le_bytes());
                out.extend_from_slice(&0u32.to_le_bytes());
            } else {
                out.extend_from_slice(&[0u8; 24]);
            }
        }
    }

    pub(crate) fn response_edid_into(&self, hdr: Option<CtrlHdr>, out: &mut Vec<u8>) {
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_EDID, hdr);
        out.extend_from_slice(&128u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        let edid = build_edid(self.width, self.height);
        out.extend_from_slice(&edid);
        out.resize(out.len() + (1024 - 128), 0);
    }
}
