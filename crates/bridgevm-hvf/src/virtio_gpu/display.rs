//! Display topology: GET_DISPLAY_INFO, host-driven resolution change, EDID generation.

use super::*;

pub(crate) mod edid;
pub(crate) use edid::*;

/// virtio-gpu config `events_read` bit: the host changed the scanout layout
/// (resolution), so the guest should re-query GET_DISPLAY_INFO/GET_EDID.
pub(crate) const VIRTIO_GPU_EVENT_DISPLAY: u32 = 1 << 0;

/// Largest scanout the resize path accepts, matching the EDID/mode range the
/// viogpu3d driver advertises. Guards the scanout allocation.
pub(crate) const MAX_SCANOUT_DIMENSION: u32 = 7680;

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
