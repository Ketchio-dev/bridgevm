//! The launch-time microphone setting decides the guest-visible layout.
//!
//! On, GCAP 0x1101: the capture stream is SD0 at 0x80 and playback SD1 at 0xa0,
//! with interrupt bits and DMA position entries in that order. Off, the guest
//! sees main's controller: GCAP 0x1001 and playback as SD0 at 0x80. Internally
//! the registers keep the microphone layout; guest offsets, the INTCTL and
//! INTSTS stream bits and the position entry are translated at the edge.

use super::*;

impl HdaController {
    /// Set the guest-visible layout from the launch-time microphone setting.
    /// Call it before the guest runs; resets keep it.
    pub fn set_microphone(&mut self, on: bool) {
        self.microphone = on;
    }

    /// Whether the guest sees the capture stream and the microphone pin.
    pub fn microphone(&self) -> bool {
        self.microphone
    }

    /// The internal register offset for a guest offset; `None` for a register
    /// main's controller does not have (it reads 0 and ignores writes).
    pub(crate) fn layout_offset(&self, offset: u64) -> Option<u64> {
        if self.microphone {
            return Some(offset);
        }
        let playback = REG_SD_CTL - REG_CAPTURE_SD_CTL;
        match offset {
            0x80..=0x9f | 0x2084..=0x2087 => Some(offset + playback),
            0xa0..=0xbf | 0x20a4..=0x20a7 => None,
            _ => Some(offset),
        }
    }

    /// INTCTL/INTSTS from the internal stream bits (capture bit 0, playback
    /// bit 1) to the guest's; without the microphone playback is bit 0.
    pub(crate) fn guest_stream_bits(&self, internal: u32) -> u32 {
        if self.microphone {
            return internal;
        }
        let streams = INTCTL_CAPTURE_STREAM | INTCTL_PLAYBACK_STREAM;
        let playback = u32::from(internal & INTCTL_PLAYBACK_STREAM != 0);
        (internal & !streams) | playback
    }

    /// The guest's INTCTL stream bits to the internal ones.
    pub(crate) fn internal_stream_bits(&self, guest: u32) -> u32 {
        if self.microphone {
            return guest;
        }
        let streams = INTCTL_CAPTURE_STREAM | INTCTL_PLAYBACK_STREAM;
        let playback = if guest & 1 != 0 {
            INTCTL_PLAYBACK_STREAM
        } else {
            0
        };
        (guest & !streams) | playback
    }

    /// The playback stream's DMA position entry: 8 bytes per stream, in
    /// descriptor order.
    pub(crate) fn playback_position_entry(&self) -> u64 {
        if self.microphone {
            PLAYBACK_POSITION_ENTRY
        } else {
            0
        }
    }
}
