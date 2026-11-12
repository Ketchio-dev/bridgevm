//! Split out of hda.rs to keep files under 850 lines.

use super::*;

#[path = "capture_dma.rs"]
mod capture_dma;
#[path = "command_dma.rs"]
mod command_dma;
#[path = "controller_debug.rs"]
mod controller_debug;
#[path = "diagnostics.rs"]
mod diagnostics;
#[path = "file_pcm_sink.rs"]
mod file_pcm_sink;
#[path = "interrupt_sources.rs"]
mod interrupt_sources;
#[path = "layout.rs"]
mod layout;
#[path = "pcm_sink.rs"]
mod pcm_sink;
#[path = "pcm_source.rs"]
mod pcm_source;
#[path = "playback_descriptor.rs"]
mod playback_descriptor;
#[path = "playback_dma.rs"]
mod playback_dma;
#[path = "playback_entry.rs"]
mod playback_entry;
#[path = "repeat_detector.rs"]
mod repeat_detector;
#[path = "stream_activity.rs"]
mod stream_activity;
#[path = "stream_control.rs"]
mod stream_control;
#[path = "stream_lifecycle.rs"]
mod stream_lifecycle;
#[path = "stream_run_trace.rs"]
mod stream_run_trace;
use diagnostics::HdaDiagnostics;
pub use diagnostics::HdaHostRecords;
pub use file_pcm_sink::FilePcmSink;
pub use pcm_sink::HdaPcmSink;
pub use pcm_source::HdaPcmSource;
use std::sync::Arc;
pub use stream_activity::HdaStreamActivity;

use std::{
    path::Path,
    time::{Duration, Instant},
};

use crate::{fwcfg::GuestMemoryMut, msix::MsixMessage};

pub const BAR_SIZE: u32 = 0x4000;
pub(crate) const MSI_CONTROLLER_VECTOR: u16 = 0;

pub const REG_GCAP: u64 = 0x00;
pub const REG_GCTL: u64 = 0x08;
pub const REG_WAKEEN: u64 = 0x0c;
pub const REG_STATESTS: u64 = 0x0e;
pub const REG_INTCTL: u64 = 0x20;
pub const REG_INTSTS: u64 = 0x24;
pub const REG_WALLCLK: u64 = 0x30;
pub const REG_CORBLBASE: u64 = 0x40;
pub const REG_CORBUBASE: u64 = 0x44;
pub const REG_CORBWP: u64 = 0x48;
pub const REG_CORBRP: u64 = 0x4a;
pub const REG_CORBCTL: u64 = 0x4c;
pub const REG_CORBSTS: u64 = 0x4d;
pub const REG_CORBSIZE: u64 = 0x4e;
pub const REG_RIRBLBASE: u64 = 0x50;
pub const REG_RIRBUBASE: u64 = 0x54;
pub const REG_RIRBWP: u64 = 0x58;
pub const REG_RINTCNT: u64 = 0x5a;
pub const REG_RIRBCTL: u64 = 0x5c;
pub const REG_RIRBSTS: u64 = 0x5d;
pub const REG_RIRBSIZE: u64 = 0x5e;
pub const REG_ICOI: u64 = 0x60;
pub const REG_ICII: u64 = 0x64;
pub const REG_ICIS: u64 = 0x68;
pub const REG_DPLBASE: u64 = 0x70;
pub const REG_DPUBASE: u64 = 0x74;

// Stream descriptors follow GCAP's order, input streams first: SD0 is the
// capture stream at 0x80 and SD1 the playback stream at 0xa0. Each LPIB has a
// read-only alias at 0x2000 above it, on its own page so a driver can map just
// the stream position into its audio engine; Windows reads it there.
pub const REG_CAPTURE_SD_CTL: u64 = 0x80;
pub const REG_CAPTURE_SD_STS: u64 = 0x83;
pub const REG_CAPTURE_SD_LPIB: u64 = 0x84;
pub const REG_CAPTURE_SD_LPIB_ALIAS: u64 = 0x2084;
pub const REG_CAPTURE_SD_CBL: u64 = 0x88;
pub const REG_CAPTURE_SD_LVI: u64 = 0x8c;
pub const REG_CAPTURE_SD_FMT: u64 = 0x92;
pub const REG_CAPTURE_SD_BDPL: u64 = 0x98;
pub const REG_CAPTURE_SD_BDPU: u64 = 0x9c;
pub const REG_SD_CTL: u64 = 0xa0;
pub const REG_SD_STS: u64 = 0xa3;
pub const REG_SD_LPIB: u64 = 0xa4;
pub const REG_SD_LPIB_ALIAS: u64 = 0x20a4;
pub const REG_SD_CBL: u64 = 0xa8;
pub const REG_SD_LVI: u64 = 0xac;
pub const REG_SD_FMT: u64 = 0xb2;
pub const REG_SD_BDPL: u64 = 0xb8;
pub const REG_SD_BDPU: u64 = 0xbc;
/// Each stream's DMA position entry is 8 bytes, in descriptor order.
pub const CAPTURE_POSITION_ENTRY: u64 = 0;
pub const PLAYBACK_POSITION_ENTRY: u64 = 8;

/// One input stream (ISS 1), one output stream (OSS 1), 64-bit addressing.
pub(crate) const GCAP_64OK_ONE_INPUT_ONE_OUTPUT: u16 = 0x1101;
/// Without the microphone: main's controller, one output stream at SD0.
pub(crate) const GCAP_64OK_ONE_OUTPUT: u16 = 0x1001;
pub(crate) const GCTL_CRST: u32 = 1;
pub(crate) const INTCTL_GIE: u32 = 1 << 31;
pub(crate) const INTCTL_CIE: u32 = 1 << 30;
/// Stream interrupt enable and status bits follow descriptor order.
pub const INTCTL_CAPTURE_STREAM: u32 = 1;
pub const INTCTL_PLAYBACK_STREAM: u32 = 1 << 1;
pub(crate) const INTSTS_GIS: u32 = 1 << 31;
pub(crate) const INTSTS_CIS: u32 = 1 << 30;
pub(crate) const CORBCTL_RUN: u8 = 1 << 1;
pub(crate) const CORBSTS_CMEI: u8 = 1;
pub(crate) const RIRBCTL_RINTCTL: u8 = 1;
pub(crate) const RIRBCTL_DMA: u8 = 1 << 1;
pub(crate) const RIRBCTL_OIC: u8 = 1 << 2;
pub(crate) const RIRBSTS_RINTFL: u8 = 1;
pub(crate) const RIRBSTS_OIS: u8 = 1 << 2;
pub(crate) const ICIS_ICB: u16 = 1;
pub(crate) const ICIS_IRV: u16 = 1 << 1;
pub(crate) const SDCTL_SRST: u32 = 1;
pub(crate) const SDCTL_RUN: u32 = 1 << 1;
pub(crate) const SDCTL_IOCE: u32 = 1 << 2;
pub(crate) const SDCTL_FEIE: u32 = 1 << 3;
pub(crate) const SDCTL_DEIE: u32 = 1 << 4;
pub(crate) const SDSTS_BCIS: u8 = 1 << 2;
pub(crate) const SDSTS_FIFOE: u8 = 1 << 3;
pub(crate) const SDSTS_DESE: u8 = 1 << 4;
pub(crate) const BDL_IOC: u32 = 1;
pub(crate) const MAX_POLL_ELAPSED: Duration = Duration::from_millis(100);
pub(crate) const TEST_POLL_QUANTUM: Duration = Duration::from_millis(10);
pub(crate) const RING_SIZE_CAPABILITIES: u8 = 0xe0; // 2, 16, and 256 entry rings.

pub(crate) const CODEC_ROOT: u8 = 0;
pub(crate) const CODEC_AFG: u8 = 1;
pub(crate) const CODEC_DAC: u8 = 2;
pub(crate) const CODEC_SPEAKER: u8 = 3;
pub(crate) const CODEC_ADC: u8 = 4;
pub(crate) const CODEC_MIC: u8 = 5;
// Stable virtual codec identity; the output path is nodes 2 -> 3 and the input
// path 5 -> 4, QEMU hda-duplex's numbering with fixed speaker and mic pins.
pub(crate) const CODEC_VENDOR_ID: u32 = 0x1af4_0022;
pub(crate) const CODEC_IMPLEMENTATION_ID: u32 = CODEC_VENDOR_ID;
pub(crate) const CODEC_REVISION_ID: u32 = 0x0010_0101;
/// Without the microphone the function group has main's nodes 2 DAC and 3 speaker.
pub(crate) const CODEC_AFG_CHILD_NODE_COUNT_NO_MIC: u32 =
    ((CODEC_DAC as u32) << 16) | (CODEC_SPEAKER - CODEC_DAC + 1) as u32;
pub(crate) const CODEC_AFG_CHILD_NODE_COUNT: u32 =
    ((CODEC_DAC as u32) << 16) | (CODEC_MIC - CODEC_DAC + 1) as u32;
pub(crate) const CODEC_AFG_CAPABILITIES: u32 = 0x0000_0808;
pub(crate) const CODEC_PCM_SIZE_RATES: u32 = 0x0002_01fc; // 16..96 kHz, signed 16-bit.
pub(crate) const CODEC_STREAM_FORMATS: u32 = 0x0000_0001; // PCM.

// The virtual output AFG exposes the parameter but no explicit Dx bits.
pub(crate) const CODEC_AFG_POWER_STATES: u32 = 0;
pub(crate) const CODEC_WIDGET_POWER_STATES: u32 = 0x0000_000f; // D0, D1, D2, and D3.
pub(crate) const CODEC_OUTPUT_AMP_CAPS: u32 = 0x8003_4a4a;
pub(crate) const DAC_WIDGET_CAPABILITIES: u32 = 0x0000_041d;
pub(crate) const SPEAKER_WIDGET_CAPABILITIES: u32 = 0x0040_0501;
pub(crate) const SPEAKER_PIN_CAPABILITIES: u32 = 0x0001_0014;
pub(crate) const SPEAKER_CONFIG_DEFAULT: u32 = 0x9017_0110;
pub(crate) const SPEAKER_PIN_SENSE: u32 = 0x8000_0000;
/// Input converter: stereo, connection list, format override, power control,
/// no amp, so Windows applies its own capture volume and mute.
pub(crate) const ADC_WIDGET_CAPABILITIES: u32 = 0x0010_0511;
/// The capture source delivers 48 kHz signed 16-bit PCM only.
pub(crate) const ADC_PCM_SIZE_RATES: u32 = 0x0002_0040;
pub(crate) const MIC_WIDGET_CAPABILITIES: u32 = 0x0040_0401;
pub(crate) const MIC_PIN_CAPABILITIES: u32 = 0x0000_0020; // input capable.
/// Fixed internal microphone, association 2, no jack detection.
pub(crate) const MIC_CONFIG_DEFAULT: u32 = 0x90a7_0120;
pub(crate) const PINCTL_IN_EN: u8 = 0x20;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StreamDescriptor {
    pub(crate) ctl: u32,
    pub(crate) sts: u8,
    pub(crate) lpib: u32,
    pub(crate) cbl: u32,
    pub(crate) lvi: u16,
    pub(crate) fmt: u16,
    pub(crate) bdl: u64,
    pub(crate) bdl_index: u16,
    pub(crate) bdl_offset: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CodecState {
    pub(crate) converter_format: u16,
    pub(crate) stream_channel: u8,
    pub(crate) pin_ctl: u8,
    pub(crate) power_state: [u8; 6],
    pub(crate) connection_select: u8,
    pub(crate) eapd: u8,
    pub(crate) dac_amp_gain_mute: [u8; 2],
    pub(crate) adc_format: u16,
    pub(crate) adc_stream_channel: u8,
    pub(crate) mic_pin_ctl: u8,
}

impl CodecState {
    pub(crate) fn new() -> Self {
        Self {
            converter_format: 0x0011, // 48 kHz, signed 16-bit, stereo.
            pin_ctl: 0x40,
            eapd: 0x02,
            dac_amp_gain_mute: [0x4a; 2],
            adc_format: 0x0011,
            mic_pin_ctl: PINCTL_IN_EN,
            ..Self::default()
        }
    }
}

/// Intel HDA register block, one capture and one playback stream, and one
/// codec with a speaker and a microphone.
pub struct HdaController {
    pub(crate) gctl: u32,
    pub(crate) wakeen: u16,
    pub(crate) statests: u16,
    pub(crate) intctl: u32,
    pub(crate) wallclk_base: Option<Instant>,
    pub(crate) corb_base: u64,
    pub(crate) corb_wp: u16,
    pub(crate) corb_rp: u16,
    pub(crate) corb_rp_reset: bool,
    pub(crate) corb_ctl: u8,
    pub(crate) corb_sts: u8,
    pub(crate) corb_size: u8,
    pub(crate) rirb_base: u64,
    pub(crate) rirb_wp: u16,
    pub(crate) rintcnt: u16,
    pub(crate) rirb_ctl: u8,
    pub(crate) rirb_sts: u8,
    pub(crate) rirb_size: u8,
    pub(crate) responses_since_irq: u16,
    pub(crate) icoi: u32,
    pub(crate) icii: u32,
    pub(crate) icis: u16,
    pub(crate) dp_base: u64,
    /// SD1, the playback stream.
    pub(crate) stream: StreamDescriptor,
    /// SD0, the capture stream (capture_dma.rs).
    pub(crate) capture: StreamDescriptor,
    pub(crate) capture_last_poll: Option<Instant>,
    pub(crate) capture_byte_time_remainder: u64,
    pub(crate) capture_scratch: Vec<u8>,
    pub(crate) pcm_source: Option<Box<dyn HdaPcmSource>>,
    /// The launch-time microphone setting. Off, the guest sees main's device
    /// register for register and verb for verb: GCAP 0x1001, playback as SD0 at
    /// 0x80 (LPIB alias 0x2084, interrupt bit 0, position entry 0) and a codec
    /// with nodes 2 and 3 only. On, the capture stream is SD0 and playback SD1.
    /// It survives controller and platform resets.
    pub(crate) microphone: bool,
    pub(crate) codec: CodecState,
    pub(crate) last_poll: Option<Instant>,
    pub(crate) byte_time_remainder: u64,
    pub(crate) pcm_sink: Option<Box<dyn HdaPcmSink>>,
    pub(crate) pcm_scratch: Vec<u8>,
    pub(crate) pcm_sink_overridden: bool,
    /// Interrupt sources (INTSTS bits) asserted at the last MSI latch.
    pub(crate) interrupt_asserted: u32,
    pub(crate) interrupt_pending: bool,
    /// The latched message was raised by playback or the controller, not
    /// capture alone: `hda poll:` msis_sent keeps counting only those.
    pub(crate) interrupt_pending_beyond_capture: bool,
    pub(crate) diagnostics: HdaDiagnostics,
    pub(crate) stream_activity: Arc<HdaStreamActivity>,
}

impl Default for HdaController {
    fn default() -> Self {
        Self::new()
    }
}

impl HdaController {
    pub fn new() -> Self {
        Self::with_pcm_output_path(std::env::var_os("BRIDGEVM_HDA_PCM_OUT"))
    }

    pub fn with_pcm_output_path<P: AsRef<Path>>(path: Option<P>) -> Self {
        let pcm_sink = path.map(|path| {
            Box::new(FilePcmSink::create(path.as_ref()).unwrap_or_else(|error| {
                panic!(
                    "failed to open BRIDGEVM_HDA_PCM_OUT {}: {error}",
                    path.as_ref().display()
                )
            })) as Box<dyn HdaPcmSink>
        });
        Self {
            gctl: 0,
            wakeen: 0,
            statests: 0,
            intctl: 0,
            wallclk_base: None,
            corb_base: 0,
            corb_wp: 0,
            corb_rp: 0,
            corb_rp_reset: false,
            corb_ctl: 0,
            corb_sts: 0,
            corb_size: RING_SIZE_CAPABILITIES | 2,
            rirb_base: 0,
            rirb_wp: 0,
            rintcnt: 1,
            rirb_ctl: 0,
            rirb_sts: 0,
            rirb_size: RING_SIZE_CAPABILITIES | 2,
            responses_since_irq: 0,
            icoi: 0,
            icii: 0,
            icis: 0,
            dp_base: 0,
            stream: StreamDescriptor::default(),
            capture: StreamDescriptor::default(),
            capture_last_poll: None,
            capture_byte_time_remainder: 0,
            capture_scratch: Vec::new(),
            pcm_source: None,
            microphone: true,
            codec: CodecState::new(),
            last_poll: None,
            byte_time_remainder: 0,
            pcm_sink,
            pcm_scratch: Vec::new(),
            pcm_sink_overridden: false,
            interrupt_asserted: 0,
            interrupt_pending: false,
            interrupt_pending_beyond_capture: false,
            diagnostics: HdaDiagnostics::default(),
            stream_activity: Arc::default(),
        }
    }

    /// Replace the current PCM destination. Passing `None` disables output.
    pub fn set_pcm_sink(&mut self, sink: Option<Box<dyn HdaPcmSink>>) {
        self.pcm_sink = sink;
        self.pcm_sink_overridden = true;
    }

    /// Reset platform runtime state, retaining an explicitly installed host
    /// sink while preserving the environment file sink's recreate/truncate
    /// behavior from before sink abstraction.
    pub(crate) fn reset_runtime_state(&mut self) {
        if self.pcm_sink_overridden {
            self.controller_reset();
        } else {
            self.replace_keeping_diagnostics(Self::new());
        }
    }

    pub fn interrupt_level(&self) -> bool {
        self.intctl & INTCTL_GIE != 0 && self.interrupt_sources() != 0
    }

    /// Latch a message when an interrupt source rises and, when the enclosing
    /// PCI function has standard MSI enabled, emit one message using the
    /// address/data programmed in PCI config space. A source rising while
    /// another is still pending gets its own message: with one message per
    /// rise of the aggregate level, a capture IOC set while the guest's handler
    /// cleared a playback IOC would leave the level high and both streams
    /// without another interrupt.
    pub fn drain_pending_msi_into(
        &mut self,
        enabled: bool,
        address: u64,
        data: u32,
        out: &mut Vec<MsixMessage>,
    ) {
        let sources = if self.intctl & INTCTL_GIE != 0 {
            self.interrupt_sources()
        } else {
            0
        };
        // Without the microphone, main's latch: one message per rise of the
        // aggregate level.
        let rising = if self.microphone {
            sources & !self.interrupt_asserted
        } else if self.interrupt_asserted == 0 {
            sources
        } else {
            0
        };
        if rising != 0 {
            self.interrupt_pending = true;
            self.interrupt_pending_beyond_capture |= rising & !INTCTL_CAPTURE_STREAM != 0;
        }
        self.interrupt_asserted = sources;

        if enabled && self.interrupt_pending {
            self.interrupt_pending = false;
            let counted = std::mem::take(&mut self.interrupt_pending_beyond_capture);
            self.diagnostics
                .note_msi(counted && self.stream.ctl & SDCTL_RUN != 0);
            out.push(MsixMessage {
                vector: MSI_CONTROLLER_VECTOR,
                address,
                data,
            });
        }
    }

    pub fn mmio_read(&self, offset: u64, size: u8) -> u64 {
        if size == 0 || size > 8 {
            return 0;
        }
        let running = self.stream.ctl & SDCTL_RUN != 0;
        if let Some(internal) = self.layout_offset(offset) {
            self.diagnostics.note_register_read(internal, size, running);
        }
        let mut value = 0;
        for byte in 0..size {
            let byte_value = self
                .layout_offset(offset + u64::from(byte))
                .map_or(0, |internal| self.read_byte(internal));
            value |= u64::from(byte_value) << (byte * 8);
        }
        if hda_trace_enabled() {
            println!("hda: mmio read off={offset:#x} size={size} value={value:#x}");
        }
        value
    }

    pub fn mmio_write(&mut self, offset: u64, size: u8, value: u64, mem: &mut dyn GuestMemoryMut) {
        if hda_trace_enabled() {
            println!("hda: mmio write off={offset:#x} size={size} value={value:#x}");
        }
        let size = size.min(8);
        let incoming = value.to_le_bytes();
        let mut written = [false; 0xc0];
        let mut bytes = [0u8; 0xc0];
        for byte in 0..size {
            let Some(absolute) = self.layout_offset(offset + u64::from(byte)) else {
                continue;
            };
            if let Ok(index) = usize::try_from(absolute) {
                if index < bytes.len() {
                    written[index] = true;
                    bytes[index] = incoming[usize::from(byte)];
                }
            }
        }

        macro_rules! rw {
            ($off:expr, $width:expr, $old:expr) => {{
                let off = $off as usize;
                let width = $width as usize;
                let mut raw = ($old as u64).to_le_bytes();
                let mut touched = false;
                for i in 0..width {
                    if written[off + i] {
                        raw[i] = bytes[off + i];
                        touched = true;
                    }
                }
                (touched, u64::from_le_bytes(raw))
            }};
        }

        let (touched, next) = rw!(REG_GCTL, 4, self.gctl);
        if touched {
            self.write_gctl(next as u32);
        }
        let (touched, next) = rw!(REG_WAKEEN, 2, self.wakeen);
        if touched {
            self.wakeen = next as u16 & 0x7fff;
        }
        let (touched, next) = rw!(REG_STATESTS, 2, 0u16);
        if touched {
            self.statests &= !(next as u16);
        }
        let (touched, next) = rw!(REG_INTCTL, 4, self.guest_stream_bits(self.intctl));
        if touched {
            self.intctl = self.internal_stream_bits(next as u32)
                & (INTCTL_GIE | INTCTL_CIE | INTCTL_CAPTURE_STREAM | INTCTL_PLAYBACK_STREAM);
        }
        let (touched, next) = rw!(REG_CORBLBASE, 4, self.corb_base as u32);
        if touched {
            self.corb_base = (self.corb_base & !0xffff_ffff) | (next as u32 as u64 & !0x7f);
        }
        let (touched, next) = rw!(REG_CORBUBASE, 4, self.corb_base >> 32);
        if touched {
            self.corb_base = (self.corb_base & 0xffff_ffff) | ((next as u32 as u64) << 32);
        }
        let (touched, next) = rw!(REG_CORBWP, 2, self.corb_wp);
        if touched {
            self.corb_wp = next as u16 & self.corb_pointer_mask();
        }
        let (touched, next) = rw!(REG_CORBRP, 2, self.corb_rp);
        if touched {
            if next & 0x8000 != 0 {
                self.corb_rp = 0;
                self.corb_rp_reset = true;
            } else {
                self.corb_rp_reset = false;
            }
        }
        let (touched, next) = rw!(REG_CORBCTL, 1, self.corb_ctl);
        if touched {
            self.corb_ctl = next as u8 & 0x03;
        }
        let (touched, next) = rw!(REG_CORBSTS, 1, 0u8);
        if touched {
            self.corb_sts &= !(next as u8);
        }
        let (touched, next) = rw!(REG_CORBSIZE, 1, self.corb_size);
        if touched && self.corb_ctl & CORBCTL_RUN == 0 {
            self.corb_size = RING_SIZE_CAPABILITIES | (next as u8 & 0x03).min(2);
            self.corb_wp &= self.corb_pointer_mask();
            self.corb_rp &= self.corb_pointer_mask();
        }
        let (touched, next) = rw!(REG_RIRBLBASE, 4, self.rirb_base as u32);
        if touched {
            self.rirb_base = (self.rirb_base & !0xffff_ffff) | (next as u32 as u64 & !0x7f);
        }
        let (touched, next) = rw!(REG_RIRBUBASE, 4, self.rirb_base >> 32);
        if touched {
            self.rirb_base = (self.rirb_base & 0xffff_ffff) | ((next as u32 as u64) << 32);
        }
        let (touched, next) = rw!(REG_RIRBWP, 2, self.rirb_wp);
        if touched && next & 0x8000 != 0 {
            self.rirb_wp = 0;
            self.responses_since_irq = 0;
        }
        let (touched, next) = rw!(REG_RINTCNT, 2, self.rintcnt);
        if touched {
            self.rintcnt = next as u16;
        }
        let (touched, next) = rw!(REG_RIRBCTL, 1, self.rirb_ctl);
        if touched {
            self.rirb_ctl = next as u8 & 0x07;
        }
        let (touched, next) = rw!(REG_RIRBSTS, 1, 0u8);
        if touched {
            self.rirb_sts &= !(next as u8);
        }
        let (touched, next) = rw!(REG_RIRBSIZE, 1, self.rirb_size);
        if touched && self.rirb_ctl & RIRBCTL_DMA == 0 {
            self.rirb_size = RING_SIZE_CAPABILITIES | (next as u8 & 0x03).min(2);
            self.rirb_wp &= self.rirb_pointer_mask();
        }
        let (touched, next) = rw!(REG_ICOI, 4, self.icoi);
        if touched {
            self.icoi = next as u32;
        }
        let (touched, next) = rw!(REG_ICIS, 2, self.icis);
        if touched {
            if next & u64::from(ICIS_IRV) != 0 {
                self.icis &= !ICIS_IRV;
            }
            if next & u64::from(ICIS_ICB) != 0 && self.icis & ICIS_ICB == 0 {
                self.icis |= ICIS_ICB;
                self.icii = self.codec_verb(self.icoi);
                self.icis = (self.icis & !ICIS_ICB) | ICIS_IRV;
            }
        }
        let (touched, next) = rw!(REG_DPLBASE, 4, self.dp_base as u32);
        if touched {
            self.dp_base = (self.dp_base & !0xffff_ffff) | (next as u32 as u64 & !0x7e);
        }
        let (touched, next) = rw!(REG_DPUBASE, 4, self.dp_base >> 32);
        if touched {
            self.dp_base = (self.dp_base & 0xffff_ffff) | ((next as u32 as u64) << 32);
        }
        let (touched, next) = rw!(REG_CAPTURE_SD_CTL, 3, self.capture.ctl);
        if touched {
            self.write_capture_ctl(next as u32 & 0x00f0_001f);
        }
        let (touched, next) = rw!(REG_CAPTURE_SD_STS, 1, 0u8);
        if touched {
            self.capture.sts &= !(next as u8 & (SDSTS_BCIS | SDSTS_FIFOE | SDSTS_DESE));
        }
        let (touched, next) = rw!(REG_CAPTURE_SD_CBL, 4, self.capture.cbl);
        if touched && self.capture.ctl & SDCTL_RUN == 0 {
            self.capture.cbl = next as u32;
        }
        let (touched, next) = rw!(REG_CAPTURE_SD_LVI, 2, self.capture.lvi);
        if touched && self.capture.ctl & SDCTL_RUN == 0 {
            self.capture.lvi = next as u16;
        }
        let (touched, next) = rw!(REG_CAPTURE_SD_FMT, 2, self.capture.fmt);
        if touched && self.capture.ctl & SDCTL_RUN == 0 {
            self.capture.fmt = next as u16 & 0x7fff;
        }
        let (touched, next) = rw!(REG_CAPTURE_SD_BDPL, 4, self.capture.bdl as u32);
        if touched && self.capture.ctl & SDCTL_RUN == 0 {
            self.capture.bdl = (self.capture.bdl & !0xffff_ffff) | (next as u32 as u64 & !0x7f);
        }
        let (touched, next) = rw!(REG_CAPTURE_SD_BDPU, 4, self.capture.bdl >> 32);
        if touched && self.capture.ctl & SDCTL_RUN == 0 {
            self.capture.bdl = (self.capture.bdl & 0xffff_ffff) | ((next as u32 as u64) << 32);
        }
        let (touched, next) = rw!(REG_SD_CTL, 3, self.stream.ctl);
        if touched {
            self.write_stream_ctl(next as u32 & 0x00f0_001f);
        }
        let (touched, next) = rw!(REG_SD_STS, 1, 0u8);
        if touched {
            self.stream.sts &= !(next as u8 & (SDSTS_BCIS | SDSTS_FIFOE | SDSTS_DESE));
        }
        let (touched, next) = rw!(REG_SD_CBL, 4, self.stream.cbl);
        if touched && self.stream.ctl & SDCTL_RUN == 0 {
            self.stream.cbl = next as u32;
        }
        let (touched, next) = rw!(REG_SD_LVI, 2, self.stream.lvi);
        if touched && self.stream.ctl & SDCTL_RUN == 0 {
            self.stream.lvi = next as u16;
        }
        let (touched, next) = rw!(REG_SD_FMT, 2, self.stream.fmt);
        if touched {
            self.write_stream_format(next as u16);
        }
        let (touched, next) = rw!(REG_SD_BDPL, 4, self.stream.bdl as u32);
        if touched && self.stream.ctl & SDCTL_RUN == 0 {
            self.stream.bdl = (self.stream.bdl & !0xffff_ffff) | (next as u32 as u64 & !0x7f);
        }
        let (touched, next) = rw!(REG_SD_BDPU, 4, self.stream.bdl >> 32);
        if touched && self.stream.ctl & SDCTL_RUN == 0 {
            self.stream.bdl = (self.stream.bdl & 0xffff_ffff) | ((next as u32 as u64) << 32);
        }

        self.process_corb(mem);
    }

    /// Advance playback and capture DMA by elapsed host time. `None` is the
    /// deterministic unit-test mode and advances one 10 ms quantum.
    pub fn poll(&mut self, mem: &mut dyn GuestMemoryMut, now: Option<Instant>) {
        self.poll_capture(mem, now);
        self.poll_playback(mem, now);
    }

    fn poll_playback(&mut self, mem: &mut dyn GuestMemoryMut, now: Option<Instant>) {
        if self.stream.ctl & SDCTL_RUN == 0 || self.gctl & GCTL_CRST == 0 {
            self.last_poll = now;
            return;
        }
        self.note_diagnostic_poll(now, mem);
        let elapsed = match now {
            Some(now) => match capture_dma::elapsed_since(&mut self.last_poll, now) {
                Some(elapsed) => elapsed,
                None => return,
            },
            None => TEST_POLL_QUANTUM,
        };
        self.poll_for_duration(mem, elapsed);
    }

    pub fn poll_for_duration(&mut self, mem: &mut dyn GuestMemoryMut, elapsed: Duration) {
        if self.stream.ctl & SDCTL_RUN == 0 || self.gctl & GCTL_CRST == 0 {
            return;
        }
        let Some(bytes_per_second) = stream_bytes_per_second(self.stream.fmt) else {
            self.stream.sts |= SDSTS_DESE;
            return;
        };
        let numerator = elapsed.as_nanos() * u128::from(bytes_per_second)
            + u128::from(self.byte_time_remainder);
        let whole_bytes = numerator / 1_000_000_000;
        let frame_bytes = usize::from(stream_frame_bytes(self.stream.fmt).unwrap_or(1));
        let budget = (whole_bytes as usize / frame_bytes) * frame_bytes;
        self.byte_time_remainder =
            (numerator % 1_000_000_000 + (whole_bytes - budget as u128) * 1_000_000_000) as u64;
        self.consume_stream(mem, budget);
    }

    pub(crate) fn write_gctl(&mut self, next: u32) {
        if next & GCTL_CRST == 0 {
            self.controller_reset();
            return;
        }
        if self.gctl & GCTL_CRST == 0 {
            self.gctl = GCTL_CRST;
            self.statests |= 1;
            // VM time: the guest's audio wall clock does not jump by a pause.
            self.wallclk_base = Some(crate::vm_clock::active_now());
            if hda_trace_enabled() {
                println!("hda: controller reset released, codec 0 present");
            }
        }
        self.gctl = (self.gctl & GCTL_CRST) | (next & 0x0000_0102);
    }

    pub(crate) fn codec_verb(&mut self, command: u32) -> u32 {
        let codec = ((command >> 28) & 0x0f) as u8;
        let nid = ((command >> 20) & 0xff) as u8;
        let payload20 = command & 0x000f_ffff;
        let last_node = if self.microphone {
            CODEC_MIC
        } else {
            CODEC_SPEAKER
        };
        if codec != 0 || nid > last_node {
            return 0;
        }
        let verb12 = ((payload20 >> 8) & 0x0fff) as u16;
        let payload8 = payload20 as u8;
        let verb4 = ((payload20 >> 16) & 0x0f) as u8;
        let payload16 = payload20 as u16;
        let response = match verb4 {
            0x2 => {
                if nid == CODEC_DAC {
                    self.codec.converter_format = payload16;
                } else if nid == CODEC_ADC {
                    self.codec.adc_format = payload16;
                }
                0
            }
            0x3 => {
                if nid == CODEC_DAC && payload16 & 0x8000 != 0 {
                    let gain_mute = payload16 as u8;
                    if payload16 & 0x2000 != 0 {
                        self.codec.dac_amp_gain_mute[0] = gain_mute;
                    }
                    if payload16 & 0x1000 != 0 {
                        self.codec.dac_amp_gain_mute[1] = gain_mute;
                    }
                }
                0
            }
            0xa if nid == CODEC_DAC => u32::from(self.codec.converter_format),
            0xa if nid == CODEC_ADC => u32::from(self.codec.adc_format),
            0xa => 0,
            0xb if nid == CODEC_DAC => {
                u32::from(self.codec.dac_amp_gain_mute[usize::from(payload16 & 0x2000 == 0)])
            }
            0xb => 0,
            _ => match verb12 {
                0xf00 => codec_parameter(nid, payload8, self.microphone),
                0xf01 if nid == CODEC_SPEAKER => u32::from(self.codec.connection_select),
                0x701 => {
                    if nid == CODEC_SPEAKER {
                        self.codec.connection_select = 0;
                    }
                    0
                }
                0xf02 if nid == CODEC_SPEAKER && payload8 == 0 => u32::from(CODEC_DAC),
                0xf02 if nid == CODEC_ADC && payload8 == 0 => u32::from(CODEC_MIC),
                0xf02 => 0,
                0xf05 if nid != CODEC_ROOT => {
                    power_state_response(self.codec.power_state[nid as usize])
                }
                0x705 => {
                    if nid != CODEC_ROOT {
                        self.codec.power_state[nid as usize] = payload8 & 0x03;
                    }
                    0
                }
                0xf06 if nid == CODEC_DAC => u32::from(self.codec.stream_channel),
                0xf06 if nid == CODEC_ADC => u32::from(self.codec.adc_stream_channel),
                0x706 => {
                    if nid == CODEC_DAC {
                        self.codec.stream_channel = payload8;
                    } else if nid == CODEC_ADC {
                        self.codec.adc_stream_channel = payload8;
                    }
                    0
                }
                0xf07 if nid == CODEC_SPEAKER => u32::from(self.codec.pin_ctl),
                0xf07 if nid == CODEC_MIC => u32::from(self.codec.mic_pin_ctl),
                0x707 => {
                    if nid == CODEC_SPEAKER {
                        self.codec.pin_ctl = payload8 & 0x40;
                    } else if nid == CODEC_MIC {
                        self.codec.mic_pin_ctl = payload8 & PINCTL_IN_EN;
                    }
                    0
                }
                0xf09 if nid == CODEC_SPEAKER || nid == CODEC_MIC => SPEAKER_PIN_SENSE,
                0xf0c if nid == CODEC_SPEAKER => u32::from(self.codec.eapd),
                0x70c => {
                    if nid == CODEC_SPEAKER {
                        self.codec.eapd = payload8 & 0x07;
                    }
                    0
                }
                0xf1c if nid == CODEC_SPEAKER => SPEAKER_CONFIG_DEFAULT,
                0xf1c if nid == CODEC_MIC => MIC_CONFIG_DEFAULT,
                // The Implementation Identification register belongs to function
                // groups.  It is a verb (F20), not GET_PARAMETER parameter 01.
                0xf20 if nid == CODEC_AFG => CODEC_IMPLEMENTATION_ID,
                0x7ff if nid == CODEC_AFG => {
                    self.codec = CodecState::new();
                    0
                }
                _ => 0,
            },
        };
        if hda_trace_enabled() {
            println!("hda: verb={command:#010x} nid={nid} response={response:#010x}");
        }
        response
    }

    pub(crate) fn intsts(&self) -> u32 {
        let sources = self.interrupt_sources();
        sources | if sources != 0 { INTSTS_GIS } else { 0 }
    }

    pub(crate) fn corb_pointer_mask(&self) -> u16 {
        ring_entries(self.corb_size) - 1
    }

    pub(crate) fn rirb_pointer_mask(&self) -> u16 {
        ring_entries(self.rirb_size) - 1
    }

    pub(crate) fn wallclk(&self) -> u32 {
        self.wallclk_base
            .map(|base| {
                let elapsed = crate::vm_clock::active_now().saturating_duration_since(base);
                (elapsed.as_nanos() * 24_000_000 / 1_000_000_000) as u32
            })
            .unwrap_or(0)
    }

    pub(crate) fn read_byte(&self, offset: u64) -> u8 {
        macro_rules! byte {
            ($off:expr, $width:expr, $value:expr) => {
                if ($off..$off + $width).contains(&offset) {
                    return (($value as u64 >> ((offset - $off) * 8)) & 0xff) as u8;
                }
            };
        }
        let gcap = if self.microphone {
            GCAP_64OK_ONE_INPUT_ONE_OUTPUT
        } else {
            GCAP_64OK_ONE_OUTPUT
        };
        byte!(REG_GCAP, 2, gcap);
        byte!(0x02, 1, 0u8); // VMIN
        byte!(0x03, 1, 1u8); // VMAJ
        byte!(0x04, 2, 0x003cu16); // OUTPAY
        let inpay: u16 = if self.microphone { 0x001d } else { 0 };
        byte!(0x06, 2, inpay); // INPAY: 29 words per frame (input bandwidth); main's 0 without the mic
        byte!(REG_GCTL, 4, self.gctl);
        byte!(REG_WAKEEN, 2, self.wakeen);
        byte!(REG_STATESTS, 2, self.statests);
        byte!(0x10, 2, 0u16); // GSTS
        byte!(REG_INTCTL, 4, self.guest_stream_bits(self.intctl));
        byte!(REG_INTSTS, 4, self.guest_stream_bits(self.intsts()));
        byte!(REG_WALLCLK, 4, self.wallclk());
        byte!(0x34, 4, 0u32); // SSYNC
        byte!(REG_CORBLBASE, 4, self.corb_base as u32);
        byte!(REG_CORBUBASE, 4, self.corb_base >> 32);
        byte!(REG_CORBWP, 2, self.corb_wp);
        byte!(
            REG_CORBRP,
            2,
            self.corb_rp | if self.corb_rp_reset { 0x8000 } else { 0 }
        );
        byte!(REG_CORBCTL, 1, self.corb_ctl);
        byte!(REG_CORBSTS, 1, self.corb_sts);
        byte!(REG_CORBSIZE, 1, self.corb_size);
        byte!(REG_RIRBLBASE, 4, self.rirb_base as u32);
        byte!(REG_RIRBUBASE, 4, self.rirb_base >> 32);
        byte!(REG_RIRBWP, 2, self.rirb_wp);
        byte!(REG_RINTCNT, 2, self.rintcnt);
        byte!(REG_RIRBCTL, 1, self.rirb_ctl);
        byte!(REG_RIRBSTS, 1, self.rirb_sts);
        byte!(REG_RIRBSIZE, 1, self.rirb_size);
        byte!(REG_ICOI, 4, self.icoi);
        byte!(REG_ICII, 4, self.icii);
        byte!(REG_ICIS, 2, self.icis);
        byte!(REG_DPLBASE, 4, self.dp_base as u32);
        byte!(REG_DPUBASE, 4, self.dp_base >> 32);
        for (base, stream) in [
            (REG_CAPTURE_SD_CTL, &self.capture),
            (REG_SD_CTL, &self.stream),
        ] {
            byte!(base, 3, stream.ctl);
            byte!(base + 0x03, 1, stream.sts);
            byte!(base + 0x04, 4, stream.lpib);
            byte!(base + 0x2004, 4, stream.lpib); // LPIB alias
            byte!(base + 0x08, 4, stream.cbl);
            byte!(base + 0x0c, 2, stream.lvi);
            byte!(base + 0x0e, 2, 0x0004u16); // FIFOW: 16-byte watermark encoding.
            byte!(base + 0x10, 2, 0x0020u16); // FIFOS: 32 bytes.
            byte!(base + 0x12, 2, stream.fmt);
            byte!(base + 0x18, 4, stream.bdl as u32);
            byte!(base + 0x1c, 4, stream.bdl >> 32);
        }
        0
    }
}
