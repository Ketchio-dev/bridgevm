//! Minimal Intel High Definition Audio controller and output codec.
//!
//! This is an output-only, host-clock-driven model of the `ich6` PCI contract
//! used by QEMU's `intel-hda` plus the playback half of `hda-duplex`.  It is
//! deliberately independent of HVF: CORB/RIRB and stream DMA use
//! [`GuestMemoryMut`], while the live platform polls [`HdaController::poll`]
//! once per VM-exit drain.  Captured samples are written as raw interleaved
//! little-endian PCM (the codec advertises 16-bit PCM only).

use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    sync::OnceLock,
    time::{Duration, Instant},
};

use crate::{fwcfg::GuestMemoryMut, msix::MsixMessage};

pub const BAR_SIZE: u32 = 0x4000;
const MSI_CONTROLLER_VECTOR: u16 = 0;

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

pub const REG_SD_CTL: u64 = 0x80;
pub const REG_SD_STS: u64 = 0x83;
pub const REG_SD_LPIB: u64 = 0x84;
pub const REG_SD_CBL: u64 = 0x88;
pub const REG_SD_LVI: u64 = 0x8c;
pub const REG_SD_FMT: u64 = 0x92;
pub const REG_SD_BDPL: u64 = 0x98;
pub const REG_SD_BDPU: u64 = 0x9c;

const GCAP_64OK_ONE_OUTPUT: u16 = 0x1001;
const GCTL_CRST: u32 = 1;
const INTCTL_GIE: u32 = 1 << 31;
const INTCTL_CIE: u32 = 1 << 30;
const INTCTL_STREAM0: u32 = 1;
const INTSTS_GIS: u32 = 1 << 31;
const INTSTS_CIS: u32 = 1 << 30;
const CORBCTL_RUN: u8 = 1 << 1;
const CORBSTS_CMEI: u8 = 1;
const RIRBCTL_RINTCTL: u8 = 1;
const RIRBCTL_DMA: u8 = 1 << 1;
const RIRBCTL_OIC: u8 = 1 << 2;
const RIRBSTS_RINTFL: u8 = 1;
const RIRBSTS_OIS: u8 = 1 << 2;
const ICIS_ICB: u16 = 1;
const ICIS_IRV: u16 = 1 << 1;
const SDCTL_SRST: u32 = 1;
const SDCTL_RUN: u32 = 1 << 1;
const SDCTL_IOCE: u32 = 1 << 2;
const SDCTL_FEIE: u32 = 1 << 3;
const SDCTL_DEIE: u32 = 1 << 4;
const SDSTS_BCIS: u8 = 1 << 2;
const SDSTS_FIFOE: u8 = 1 << 3;
const SDSTS_DESE: u8 = 1 << 4;
const BDL_IOC: u32 = 1;
const MAX_POLL_ELAPSED: Duration = Duration::from_millis(100);
const TEST_POLL_QUANTUM: Duration = Duration::from_millis(10);
const RING_SIZE_CAPABILITIES: u8 = 0xe0; // 2, 16, and 256 entry rings.

const CODEC_ROOT: u8 = 0;
const CODEC_AFG: u8 = 1;
const CODEC_DAC: u8 = 2;
const CODEC_SPEAKER: u8 = 3;
// QEMU's hda-duplex codec identity (the output path is nodes 2 -> 3).
const CODEC_VENDOR_ID: u32 = 0x1af4_0022;
const CODEC_IMPLEMENTATION_ID: u32 = CODEC_VENDOR_ID;
const CODEC_REVISION_ID: u32 = 0x0010_0101;
const CODEC_AFG_CHILD_NODE_COUNT: u32 =
    ((CODEC_DAC as u32) << 16) | (CODEC_SPEAKER - CODEC_DAC + 1) as u32;
const CODEC_AFG_CAPABILITIES: u32 = 0x0000_0808;
const CODEC_PCM_SIZE_RATES: u32 = 0x0002_01fc; // QEMU: 16..96 kHz, signed 16-bit.
const CODEC_STREAM_FORMATS: u32 = 0x0000_0001; // PCM.

// QEMU's output AFG exposes the required parameter but no explicit Dx bits.
const CODEC_AFG_POWER_STATES: u32 = 0;
const CODEC_WIDGET_POWER_STATES: u32 = 0x0000_000f; // D0, D1, D2, and D3.
const CODEC_OUTPUT_AMP_CAPS: u32 = 0x8003_4a4a;
const DAC_WIDGET_CAPABILITIES: u32 = 0x0000_041d;
const SPEAKER_WIDGET_CAPABILITIES: u32 = 0x0040_0501;
const SPEAKER_PIN_CAPABILITIES: u32 = 0x0001_0014;
const SPEAKER_CONFIG_DEFAULT: u32 = 0x9017_0110;
const SPEAKER_PIN_SENSE: u32 = 0x8000_0000;

#[derive(Debug, Clone, Copy, Default)]
struct StreamDescriptor {
    ctl: u32,
    sts: u8,
    lpib: u32,
    cbl: u32,
    lvi: u16,
    fmt: u16,
    bdl: u64,
    bdl_index: u16,
    bdl_offset: u32,
}

impl StreamDescriptor {
    fn reset_runtime(&mut self) {
        self.sts = 0;
        self.lpib = 0;
        self.bdl_index = 0;
        self.bdl_offset = 0;
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct CodecState {
    converter_format: u16,
    stream_channel: u8,
    pin_ctl: u8,
    power_state: [u8; 4],
    connection_select: u8,
    eapd: u8,
    dac_amp_gain_mute: [u8; 2],
}

impl CodecState {
    fn new() -> Self {
        Self {
            converter_format: 0x0011, // 48 kHz, signed 16-bit, stereo.
            pin_ctl: 0x40,
            eapd: 0x02,
            dac_amp_gain_mute: [0x4a; 2],
            ..Self::default()
        }
    }
}

/// Intel HDA register block, one playback stream, and one speaker codec.
pub struct HdaController {
    gctl: u32,
    wakeen: u16,
    statests: u16,
    intctl: u32,
    wallclk_base: Option<Instant>,
    corb_base: u64,
    corb_wp: u16,
    corb_rp: u16,
    corb_rp_reset: bool,
    corb_ctl: u8,
    corb_sts: u8,
    corb_size: u8,
    rirb_base: u64,
    rirb_wp: u16,
    rintcnt: u16,
    rirb_ctl: u8,
    rirb_sts: u8,
    rirb_size: u8,
    responses_since_irq: u16,
    icoi: u32,
    icii: u32,
    icis: u16,
    dp_base: u64,
    stream: StreamDescriptor,
    codec: CodecState,
    last_poll: Option<Instant>,
    byte_time_remainder: u64,
    pcm_out: Option<File>,
    interrupt_asserted: bool,
    interrupt_pending: bool,
}

impl std::fmt::Debug for HdaController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HdaController")
            .field("gctl", &self.gctl)
            .field("statests", &self.statests)
            .field("intctl", &self.intctl)
            .field("corb_base", &self.corb_base)
            .field("corb_wp", &self.corb_wp)
            .field("corb_rp", &self.corb_rp)
            .field("rirb_base", &self.rirb_base)
            .field("rirb_wp", &self.rirb_wp)
            .field("stream", &self.stream)
            .field("pcm_out", &self.pcm_out.is_some())
            .field("interrupt_asserted", &self.interrupt_asserted)
            .field("interrupt_pending", &self.interrupt_pending)
            .finish()
    }
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
        let pcm_out = path.map(|path| {
            OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(path.as_ref())
                .unwrap_or_else(|error| {
                    panic!(
                        "failed to open BRIDGEVM_HDA_PCM_OUT {}: {error}",
                        path.as_ref().display()
                    )
                })
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
            codec: CodecState::new(),
            last_poll: None,
            byte_time_remainder: 0,
            pcm_out,
            interrupt_asserted: false,
            interrupt_pending: false,
        }
    }

    pub fn interrupt_level(&self) -> bool {
        self.intctl & INTCTL_GIE != 0 && self.interrupt_sources() != 0
    }

    /// Latch the controller's single aggregate interrupt source and, when the
    /// enclosing PCI function has standard MSI enabled, emit one message using
    /// the address/data programmed in PCI config space.
    pub fn drain_pending_msi_into(
        &mut self,
        enabled: bool,
        address: u64,
        data: u32,
        out: &mut Vec<MsixMessage>,
    ) {
        let active = self.interrupt_level();
        if active && !self.interrupt_asserted {
            self.interrupt_pending = true;
        }
        self.interrupt_asserted = active;

        if enabled && self.interrupt_pending {
            self.interrupt_pending = false;
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
        let mut value = 0;
        for byte in 0..size {
            value |= u64::from(self.read_byte(offset + u64::from(byte))) << (byte * 8);
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
        let mut written = [false; 0xa0];
        let mut bytes = [0u8; 0xa0];
        for byte in 0..size {
            let absolute = offset + u64::from(byte);
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
        let (touched, next) = rw!(REG_INTCTL, 4, self.intctl);
        if touched {
            self.intctl = next as u32 & (INTCTL_GIE | INTCTL_CIE | INTCTL_STREAM0);
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
        if touched && self.stream.ctl & SDCTL_RUN == 0 {
            self.stream.fmt = next as u16 & 0x7fff;
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

    /// Consume playback DMA according to elapsed host time. `None` is the
    /// deterministic unit-test mode and advances one 10 ms quantum.
    pub fn poll(&mut self, mem: &mut dyn GuestMemoryMut, now: Option<Instant>) {
        if self.stream.ctl & SDCTL_RUN == 0 || self.gctl & GCTL_CRST == 0 {
            self.last_poll = now;
            return;
        }
        let elapsed = match now {
            Some(now) => match self.last_poll.replace(now) {
                Some(last) => now.saturating_duration_since(last).min(MAX_POLL_ELAPSED),
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

    fn write_gctl(&mut self, next: u32) {
        if next & GCTL_CRST == 0 {
            self.controller_reset();
            return;
        }
        if self.gctl & GCTL_CRST == 0 {
            self.gctl = GCTL_CRST;
            self.statests |= 1;
            self.wallclk_base = Some(Instant::now());
            if hda_trace_enabled() {
                println!("hda: controller reset released, codec 0 present");
            }
        }
        self.gctl = (self.gctl & GCTL_CRST) | (next & 0x0000_0102);
    }

    fn controller_reset(&mut self) {
        let pcm_out = self.pcm_out.take();
        *self = Self::with_pcm_output_path::<&Path>(None);
        self.pcm_out = pcm_out;
    }

    fn write_stream_ctl(&mut self, next: u32) {
        let was_running = self.stream.ctl & SDCTL_RUN != 0;
        if next & SDCTL_SRST != 0 {
            let format = self.stream.fmt;
            let bdl = self.stream.bdl;
            let cbl = self.stream.cbl;
            let lvi = self.stream.lvi;
            self.stream = StreamDescriptor {
                ctl: SDCTL_SRST,
                fmt: format,
                bdl,
                cbl,
                lvi,
                ..StreamDescriptor::default()
            };
            self.last_poll = None;
            self.byte_time_remainder = 0;
            return;
        }
        self.stream.ctl = next & !SDCTL_SRST;
        let running = self.stream.ctl & SDCTL_RUN != 0;
        if running && !was_running {
            self.stream.reset_runtime();
            self.last_poll = None;
            self.byte_time_remainder = 0;
            if hda_trace_enabled() {
                println!(
                    "hda: stream run fmt={:#06x} rate={}Hz frame={} BDL={:#x} CBL={} LVI={}",
                    self.stream.fmt,
                    stream_sample_rate(self.stream.fmt).unwrap_or(0),
                    stream_frame_bytes(self.stream.fmt).unwrap_or(0),
                    self.stream.bdl,
                    self.stream.cbl,
                    self.stream.lvi
                );
            }
        } else if !running {
            self.last_poll = None;
        }
    }

    fn process_corb(&mut self, mem: &mut dyn GuestMemoryMut) {
        if self.gctl & GCTL_CRST == 0
            || self.corb_ctl & CORBCTL_RUN == 0
            || self.rirb_ctl & RIRBCTL_DMA == 0
        {
            return;
        }
        let mask = self.corb_pointer_mask();
        let mut guard = 0usize;
        while self.corb_rp != self.corb_wp && guard < usize::from(mask) + 1 {
            let next = self.corb_rp.wrapping_add(1) & mask;
            let mut raw = [0u8; 4];
            if !mem.read_into(self.corb_base + u64::from(next) * 4, &mut raw) {
                self.corb_sts |= CORBSTS_CMEI;
                break;
            }
            let verb = u32::from_le_bytes(raw);
            let response = self.codec_verb(verb);
            self.corb_rp = next;
            if !self.push_rirb(mem, response, (verb >> 28) as u8) {
                break;
            }
            guard += 1;
        }
    }

    fn push_rirb(&mut self, mem: &mut dyn GuestMemoryMut, response: u32, codec: u8) -> bool {
        let next = self.rirb_wp.wrapping_add(1) & self.rirb_pointer_mask();
        let mut entry = [0u8; 8];
        entry[..4].copy_from_slice(&response.to_le_bytes());
        entry[4..].copy_from_slice(&u32::from(codec & 0x0f).to_le_bytes());
        if !mem.write_bytes(self.rirb_base + u64::from(next) * 8, &entry) {
            self.rirb_sts |= RIRBSTS_OIS;
            return false;
        }
        self.rirb_wp = next;
        self.responses_since_irq = self.responses_since_irq.wrapping_add(1);
        let threshold = if self.rintcnt == 0 { 256 } else { self.rintcnt };
        if self.responses_since_irq >= threshold {
            self.rirb_sts |= RIRBSTS_RINTFL;
            self.responses_since_irq = 0;
        }
        if hda_trace_enabled() {
            println!("hda: verb response codec={codec} response={response:#010x} rirb_wp={next}");
        }
        true
    }

    fn consume_stream(&mut self, mem: &mut dyn GuestMemoryMut, mut budget: usize) {
        if budget == 0 || self.stream.cbl == 0 || self.stream.bdl == 0 {
            return;
        }
        while budget > 0 && self.stream.ctl & SDCTL_RUN != 0 {
            let descriptor_gpa = self.stream.bdl + u64::from(self.stream.bdl_index) * 16;
            let mut raw = [0u8; 16];
            if !mem.read_into(descriptor_gpa, &mut raw) {
                self.stream.sts |= SDSTS_DESE;
                self.stream.ctl &= !SDCTL_RUN;
                break;
            }
            let address = u64::from_le_bytes(raw[..8].try_into().unwrap());
            let length = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let flags = u32::from_le_bytes(raw[12..16].try_into().unwrap());
            if length == 0 {
                self.stream.sts |= SDSTS_DESE;
                self.stream.ctl &= !SDCTL_RUN;
                break;
            }
            if self.stream.bdl_offset >= length {
                self.complete_bdl_entry(flags);
                continue;
            }
            let remaining_entry = (length - self.stream.bdl_offset) as usize;
            let remaining_cbl = (self.stream.cbl - self.stream.lpib.min(self.stream.cbl)) as usize;
            let chunk_len = budget.min(remaining_entry).min(remaining_cbl);
            if chunk_len == 0 {
                self.wrap_cyclic_buffer();
                self.write_position_buffer(mem);
                continue;
            }
            let Some(bytes) =
                mem.read_bytes(address + u64::from(self.stream.bdl_offset), chunk_len)
            else {
                self.stream.sts |= SDSTS_DESE;
                self.stream.ctl &= !SDCTL_RUN;
                break;
            };
            if let Some(output) = self.pcm_out.as_mut() {
                if let Err(error) = output.write_all(&bytes) {
                    eprintln!("hda: disabling PCM capture after write error: {error}");
                    self.pcm_out = None;
                }
            }
            self.stream.bdl_offset += chunk_len as u32;
            self.stream.lpib += chunk_len as u32;
            budget -= chunk_len;
            self.write_position_buffer(mem);
            if self.stream.bdl_offset == length {
                self.complete_bdl_entry(flags);
            }
            if self.stream.lpib >= self.stream.cbl {
                self.wrap_cyclic_buffer();
                self.write_position_buffer(mem);
            }
        }
    }

    fn complete_bdl_entry(&mut self, flags: u32) {
        if flags & BDL_IOC != 0 {
            self.stream.sts |= SDSTS_BCIS;
            if hda_trace_enabled() {
                println!(
                    "hda: BDL IOC index={} lpib={}",
                    self.stream.bdl_index, self.stream.lpib
                );
            }
        }
        self.stream.bdl_offset = 0;
        self.stream.bdl_index = if self.stream.bdl_index >= self.stream.lvi {
            0
        } else {
            self.stream.bdl_index + 1
        };
    }

    fn wrap_cyclic_buffer(&mut self) {
        self.stream.lpib = 0;
        self.stream.bdl_index = 0;
        self.stream.bdl_offset = 0;
    }

    fn write_position_buffer(&self, mem: &mut dyn GuestMemoryMut) {
        if self.dp_base & 1 == 0 {
            return;
        }
        let base = self.dp_base & !0x7f;
        let _ = mem.write_bytes(base, &self.stream.lpib.to_le_bytes());
    }

    fn codec_verb(&mut self, command: u32) -> u32 {
        let codec = ((command >> 28) & 0x0f) as u8;
        let nid = ((command >> 20) & 0xff) as u8;
        let payload20 = command & 0x000f_ffff;
        if codec != 0 || !matches!(nid, CODEC_ROOT | CODEC_AFG | CODEC_DAC | CODEC_SPEAKER) {
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
            0xa => (nid == CODEC_DAC)
                .then_some(u32::from(self.codec.converter_format))
                .unwrap_or(0),
            0xb => (nid == CODEC_DAC)
                .then_some(u32::from(
                    self.codec.dac_amp_gain_mute[usize::from(payload16 & 0x2000 == 0)],
                ))
                .unwrap_or(0),
            _ => match verb12 {
                0xf00 => codec_parameter(nid, payload8),
                0xf01 if nid == CODEC_SPEAKER => u32::from(self.codec.connection_select),
                0x701 => {
                    if nid == CODEC_SPEAKER {
                        self.codec.connection_select = 0;
                    }
                    0
                }
                0xf02 => (nid == CODEC_SPEAKER && payload8 == 0)
                    .then_some(u32::from(CODEC_DAC))
                    .unwrap_or(0),
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
                0x706 => {
                    if nid == CODEC_DAC {
                        self.codec.stream_channel = payload8;
                    }
                    0
                }
                0xf07 if nid == CODEC_SPEAKER => u32::from(self.codec.pin_ctl),
                0x707 => {
                    if nid == CODEC_SPEAKER {
                        self.codec.pin_ctl = payload8 & 0x40;
                    }
                    0
                }
                0xf09 if nid == CODEC_SPEAKER => SPEAKER_PIN_SENSE,
                0xf0c if nid == CODEC_SPEAKER => u32::from(self.codec.eapd),
                0x70c => {
                    if nid == CODEC_SPEAKER {
                        self.codec.eapd = payload8 & 0x07;
                    }
                    0
                }
                0xf1c if nid == CODEC_SPEAKER => SPEAKER_CONFIG_DEFAULT,
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

    fn interrupt_sources(&self) -> u32 {
        let mut sources = 0;
        let controller_pending = (self.rirb_sts & RIRBSTS_RINTFL != 0
            && self.rirb_ctl & RIRBCTL_RINTCTL != 0)
            || (self.rirb_sts & RIRBSTS_OIS != 0 && self.rirb_ctl & RIRBCTL_OIC != 0)
            || self.corb_sts & CORBSTS_CMEI != 0;
        if controller_pending && self.intctl & INTCTL_CIE != 0 {
            sources |= INTSTS_CIS;
        }
        let stream_pending = (self.stream.sts & SDSTS_BCIS != 0
            && self.stream.ctl & SDCTL_IOCE != 0)
            || (self.stream.sts & SDSTS_FIFOE != 0 && self.stream.ctl & SDCTL_FEIE != 0)
            || (self.stream.sts & SDSTS_DESE != 0 && self.stream.ctl & SDCTL_DEIE != 0);
        if stream_pending && self.intctl & INTCTL_STREAM0 != 0 {
            sources |= INTCTL_STREAM0;
        }
        sources
    }

    fn intsts(&self) -> u32 {
        let sources = self.interrupt_sources();
        sources | (sources != 0).then_some(INTSTS_GIS).unwrap_or(0)
    }

    fn corb_pointer_mask(&self) -> u16 {
        ring_entries(self.corb_size) - 1
    }

    fn rirb_pointer_mask(&self) -> u16 {
        ring_entries(self.rirb_size) - 1
    }

    fn wallclk(&self) -> u32 {
        self.wallclk_base
            .map(|base| (base.elapsed().as_nanos() * 24_000_000 / 1_000_000_000) as u32)
            .unwrap_or(0)
    }

    fn read_byte(&self, offset: u64) -> u8 {
        macro_rules! byte {
            ($off:expr, $width:expr, $value:expr) => {
                if ($off..$off + $width).contains(&offset) {
                    return (($value as u64 >> ((offset - $off) * 8)) & 0xff) as u8;
                }
            };
        }
        byte!(REG_GCAP, 2, GCAP_64OK_ONE_OUTPUT);
        byte!(0x02, 1, 0u8); // VMIN
        byte!(0x03, 1, 1u8); // VMAJ
        byte!(0x04, 2, 0x003cu16); // OUTPAY
        byte!(0x06, 2, 0u16); // INPAY
        byte!(REG_GCTL, 4, self.gctl);
        byte!(REG_WAKEEN, 2, self.wakeen);
        byte!(REG_STATESTS, 2, self.statests);
        byte!(0x10, 2, 0u16); // GSTS
        byte!(REG_INTCTL, 4, self.intctl);
        byte!(REG_INTSTS, 4, self.intsts());
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
        byte!(REG_SD_CTL, 3, self.stream.ctl);
        byte!(REG_SD_STS, 1, self.stream.sts);
        byte!(REG_SD_LPIB, 4, self.stream.lpib);
        byte!(REG_SD_CBL, 4, self.stream.cbl);
        byte!(REG_SD_LVI, 2, self.stream.lvi);
        byte!(0x8e, 2, 0x0004u16); // FIFOW: 16-byte watermark encoding.
        byte!(0x90, 2, 0x0020u16); // FIFOS: 32 bytes.
        byte!(REG_SD_FMT, 2, self.stream.fmt);
        byte!(REG_SD_BDPL, 4, self.stream.bdl as u32);
        byte!(REG_SD_BDPU, 4, self.stream.bdl >> 32);
        0
    }
}

fn codec_parameter(nid: u8, parameter: u8) -> u32 {
    if nid == CODEC_AFG {
        return afg_parameter(parameter).unwrap_or(0);
    }
    match (nid, parameter) {
        (CODEC_ROOT, 0x00) => CODEC_VENDOR_ID,
        // AC_PAR_SUBSYSTEM_ID (0x01). QEMU's output codec exposes it on the
        // root AND the audio function group; hdaudio.sys queries it during
        // enumeration and treats a missing/zero value as an invalid codec,
        // aborting before it reads the AFG's SUBORDINATE_NODE_COUNT (0x04) —
        // which is exactly the "reads AFG basics then never descends" wall.
        (CODEC_ROOT, 0x01) => CODEC_IMPLEMENTATION_ID,
        (CODEC_ROOT, 0x02) => CODEC_REVISION_ID,
        (CODEC_ROOT, 0x04) => 0x0001_0001, // node 1, one function group.
        (CODEC_DAC, 0x0a) => CODEC_PCM_SIZE_RATES,
        (CODEC_DAC, 0x0b) => CODEC_STREAM_FORMATS,
        (CODEC_DAC, 0x0f) | (CODEC_SPEAKER, 0x0f) => CODEC_WIDGET_POWER_STATES,
        (CODEC_DAC, 0x09) => DAC_WIDGET_CAPABILITIES,
        (CODEC_DAC, 0x0d) => 0,
        (CODEC_DAC, 0x12) => CODEC_OUTPUT_AMP_CAPS,
        (CODEC_SPEAKER, 0x09) => SPEAKER_WIDGET_CAPABILITIES,
        (CODEC_SPEAKER, 0x0c) => SPEAKER_PIN_CAPABILITIES,
        (CODEC_SPEAKER, 0x0d | 0x12) => 0,
        (CODEC_SPEAKER, 0x0e) => 1, // one short-form connection.
        _ => 0,
    }
}

fn afg_parameter(parameter: u8) -> Option<u32> {
    Some(match parameter {
        0x01 => CODEC_IMPLEMENTATION_ID,    // AC_PAR_SUBSYSTEM_ID (QEMU AFG has it)
        0x04 => CODEC_AFG_CHILD_NODE_COUNT, // NID 2 DAC, NID 3 pin
        0x05 => 0x0000_0001,                // audio function group
        0x08 => CODEC_AFG_CAPABILITIES,
        0x0a => CODEC_PCM_SIZE_RATES,
        0x0b => CODEC_STREAM_FORMATS,
        0x0d => 0, // default input amp capabilities: none
        0x0f => CODEC_AFG_POWER_STATES,
        0x11 => 0, // GPIO count: none
        0x12 => 0, // default output amp capabilities: none
        _ => return None,
    })
}

fn power_state_response(state: u8) -> u32 {
    u32::from(state) | (u32::from(state) << 4)
}

fn ring_entries(size: u8) -> u16 {
    match size & 0x03 {
        0 => 2,
        1 => 16,
        _ => 256,
    }
}

fn stream_sample_rate(fmt: u16) -> Option<u32> {
    if fmt & 0x8000 != 0 {
        return None;
    }
    let base = if fmt & 0x4000 != 0 { 44_100 } else { 48_000 };
    let multiplier = u32::from((fmt >> 11) & 0x7) + 1;
    let divisor = u32::from((fmt >> 8) & 0x7) + 1;
    Some(base * multiplier / divisor)
}

fn stream_frame_bytes(fmt: u16) -> Option<u16> {
    // The codec advertises 16-bit PCM only, which keeps the capture file's
    // promised raw s16le format true even if a guest programs an invalid FMT.
    let sample_bytes = ((fmt >> 4) & 0x7 == 1).then_some(2)?;
    let channels = (fmt & 0x0f) + 1;
    Some(sample_bytes * channels)
}

fn stream_bytes_per_second(fmt: u16) -> Option<u64> {
    Some(u64::from(stream_sample_rate(fmt)?) * u64::from(stream_frame_bytes(fmt)?))
}

fn hda_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("BRIDGEVM_TRACE_HDA")
            .ok()
            .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes" | "on"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform_virt::FlatGuestRam;
    use std::{fs, path::PathBuf};

    const RAM_BASE: u64 = 0x1000_0000;

    fn write(ctrl: &mut HdaController, mem: &mut FlatGuestRam, off: u64, size: u8, value: u64) {
        ctrl.mmio_write(off, size, value, mem);
    }

    fn verb(codec: u8, nid: u8, verb: u16, payload: u8) -> u32 {
        (u32::from(codec) << 28)
            | (u32::from(nid) << 20)
            | (u32::from(verb) << 8)
            | u32::from(payload)
    }

    fn verb16(codec: u8, nid: u8, verb: u8, payload: u16) -> u32 {
        (u32::from(codec) << 28)
            | (u32::from(nid) << 20)
            | (u32::from(verb) << 16)
            | u32::from(payload)
    }

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "bridgevm-hda-{label}-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }

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
            (0x000f_0000, CODEC_VENDOR_ID),          // AC_PAR_VENDOR_ID
            (0x000f_0001, CODEC_IMPLEMENTATION_ID),  // root AC_PAR_SUBSYSTEM_ID
            (0x000f_0002, CODEC_REVISION_ID),        // AC_PAR_REV_ID
            (0x000f_0004, 0x0001_0001),              // AC_PAR_NODE_COUNT
            (0x001f_0001, CODEC_IMPLEMENTATION_ID),  // AFG AC_PAR_SUBSYSTEM_ID
            (0x001f_0005, 0x0000_0001),              // AC_PAR_FUNCTION_TYPE=audio
            (0x001f_0500, 0),                        // AFG GET_POWER_STATE (D0)
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
}
