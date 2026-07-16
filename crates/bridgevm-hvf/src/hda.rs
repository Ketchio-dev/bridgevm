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

use crate::{
    fwcfg::GuestMemoryMut,
    msix::{MsixMessage, MsixTable},
};

pub const BAR_SIZE: u32 = 0x4000;
/// HDA exposes a single message because its native register model has one
/// aggregate interrupt output and no guest-programmable source/vector mapping.
pub const MSIX_VECTOR_COUNT: u16 = 1;
pub const MSIX_TABLE_OFFSET: u32 = 0x0000;
pub const MSIX_PBA_OFFSET: u32 = 0x0800;

const MSIX_CONTROLLER_VECTOR: u16 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HdaPciOp {
    Read { size: u8 },
    Write { size: u8, value: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HdaPciResult {
    ReadValue(u64),
    WriteAck,
}

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
const CODEC_REVISION_ID: u32 = 0x0010_0100;
const CODEC_PCM_SIZE_RATES: u32 = 0x0002_01fc; // QEMU: 16..96 kHz, signed 16-bit.
const SPEAKER_CONFIG_DEFAULT: u32 = 0x9017_0110;

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
    power_state: u8,
    connection_select: u8,
    eapd: u8,
}

impl CodecState {
    fn new() -> Self {
        Self {
            pin_ctl: 0x40,
            power_state: 0,
            eapd: 0x02,
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
    msix: MsixTable,
    asserted_msix_vectors: u8,
    pending_msix_vectors: u8,
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
            .field("asserted_msix_vectors", &self.asserted_msix_vectors)
            .field("pending_msix_vectors", &self.pending_msix_vectors)
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
            msix: MsixTable::new(MSIX_VECTOR_COUNT),
            asserted_msix_vectors: 0,
            pending_msix_vectors: 0,
        }
    }

    pub fn interrupt_level(&self) -> bool {
        self.intctl & INTCTL_GIE != 0 && self.interrupt_sources() != 0
    }

    pub fn msix_bar_access(&mut self, offset: u64, op: HdaPciOp) -> HdaPciResult {
        if let Some(table_offset) = self.msix_table_offset(offset) {
            return match op {
                HdaPciOp::Read { size } => {
                    HdaPciResult::ReadValue(self.msix.table_read(table_offset, size))
                }
                HdaPciOp::Write { size, value } => {
                    self.msix.table_write(table_offset, size, value);
                    HdaPciResult::WriteAck
                }
            };
        }
        if let Some(pba_offset) = self.msix_pba_offset(offset) {
            return match op {
                HdaPciOp::Read { size } => {
                    HdaPciResult::ReadValue(self.msix.pba_read(pba_offset, size))
                }
                HdaPciOp::Write { size, value } => {
                    self.msix.pba_write(pba_offset, size, value);
                    HdaPciResult::WriteAck
                }
            };
        }
        match op {
            HdaPciOp::Read { .. } => HdaPciResult::ReadValue(0),
            HdaPciOp::Write { .. } => HdaPciResult::WriteAck,
        }
    }

    pub fn drain_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        self.latch_pending_msix_vectors();
        let start = out.len();
        self.msix
            .drain_pending_into(function_enabled, function_masked, out);
        for message in &out[start..] {
            self.pending_msix_vectors &= !(1u8 << message.vector);
        }

        let mut pending = self.pending_msix_vectors;
        while pending != 0 {
            let vector = pending.trailing_zeros() as u16;
            if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                self.pending_msix_vectors &= !(1u8 << vector);
                out.push(message);
            }
            pending &= !(1u8 << vector);
        }
    }

    fn latch_pending_msix_vectors(&mut self) {
        let active = self.active_msix_vectors();
        self.pending_msix_vectors |= active & !self.asserted_msix_vectors;
        self.asserted_msix_vectors = active;
    }

    fn active_msix_vectors(&self) -> u8 {
        if self.intctl & INTCTL_GIE == 0 {
            return 0;
        }
        (self.interrupt_sources() != 0)
            .then_some(1 << MSIX_CONTROLLER_VECTOR)
            .unwrap_or(0)
    }

    fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(MSIX_TABLE_OFFSET))?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(MSIX_PBA_OFFSET))?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
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
        // CRST resets the HDA register block, not the enclosing PCI function:
        // keep the guest-programmed MSI-X table across controller resets.
        let msix = std::mem::replace(&mut self.msix, MsixTable::new(MSIX_VECTOR_COUNT));
        *self = Self::with_pcm_output_path::<&Path>(None);
        self.pcm_out = pcm_out;
        self.msix = msix;
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
        if codec != 0 {
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
            0x3 => 0, // SET_AMP_GAIN_MUTE: accepted, fixed-gain sink.
            0xa => (nid == CODEC_DAC)
                .then_some(u32::from(self.codec.converter_format))
                .unwrap_or(0),
            0xb => 0, // GET_AMP_GAIN_MUTE: unmuted, 0 dB stub.
            _ => match verb12 {
                0xf00 => codec_parameter(nid, payload8),
                0xf01 => u32::from(self.codec.connection_select),
                0x701 => {
                    self.codec.connection_select = payload8;
                    0
                }
                0xf02 => (nid == CODEC_SPEAKER)
                    .then_some(u32::from(CODEC_DAC))
                    .unwrap_or(0),
                0xf05 => {
                    u32::from(self.codec.power_state) | (u32::from(self.codec.power_state) << 4)
                }
                0x705 => {
                    self.codec.power_state = payload8 & 0x0f;
                    0
                }
                0xf06 => u32::from(self.codec.stream_channel),
                0x706 => {
                    self.codec.stream_channel = payload8;
                    0
                }
                0xf07 => u32::from(self.codec.pin_ctl),
                0x707 => {
                    self.codec.pin_ctl = payload8;
                    0
                }
                0xf09 => 0, // Pin sense: fixed-function speaker, not a jack.
                0xf0c => u32::from(self.codec.eapd),
                0x70c => {
                    self.codec.eapd = payload8;
                    0
                }
                0xf1c..=0xf1f if nid == CODEC_SPEAKER => {
                    SPEAKER_CONFIG_DEFAULT >> (u32::from(verb12 - 0xf1c) * 8)
                }
                0xf20 => CODEC_VENDOR_ID,
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
    match (nid, parameter) {
        (CODEC_ROOT, 0x00) => CODEC_VENDOR_ID,
        (CODEC_ROOT, 0x01) => CODEC_VENDOR_ID,
        (CODEC_ROOT, 0x02) => CODEC_REVISION_ID,
        (CODEC_ROOT, 0x04) => 0x0001_0001, // node 1, one function group.
        (CODEC_AFG, 0x04) => 0x0002_0002,  // nodes 2..=3.
        (CODEC_AFG, 0x05) => 0x0000_0001,  // audio function group.
        (CODEC_AFG, 0x01) => CODEC_VENDOR_ID,
        (CODEC_AFG, 0x08) => 0x0000_0808,
        (CODEC_AFG, 0x0a) | (CODEC_DAC, 0x0a) => CODEC_PCM_SIZE_RATES,
        (CODEC_AFG, 0x0b) | (CODEC_DAC, 0x0b) => 1, // PCM stream format.
        (CODEC_AFG, 0x0f) | (CODEC_DAC, 0x0f) | (CODEC_SPEAKER, 0x0f) => 1,
        (CODEC_DAC, 0x09) => 0x0000_0011, // stereo output converter, format override.
        (CODEC_SPEAKER, 0x09) => 0x0040_0101, // stereo pin + connection list.
        (CODEC_SPEAKER, 0x0c) => 0x0001_0010, // output + EAPD.
        (CODEC_SPEAKER, 0x0e) => 1,       // one short-form connection.
        _ => 0,
    }
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

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "bridgevm-hda-{label}-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }

    fn program_msix_vector(
        ctrl: &mut HdaController,
        vector: u16,
        address: u64,
        data: u32,
        masked: bool,
    ) {
        let offset = u64::from(vector) * MsixTable::ENTRY_BYTES;
        assert_eq!(
            ctrl.msix_bar_access(
                offset,
                HdaPciOp::Write {
                    size: 8,
                    value: address,
                },
            ),
            HdaPciResult::WriteAck
        );
        assert_eq!(
            ctrl.msix_bar_access(
                offset + 8,
                HdaPciOp::Write {
                    size: 4,
                    value: u64::from(data),
                },
            ),
            HdaPciResult::WriteAck
        );
        assert_eq!(
            ctrl.msix_bar_access(
                offset + 12,
                HdaPciOp::Write {
                    size: 4,
                    value: u64::from(masked),
                },
            ),
            HdaPciResult::WriteAck
        );
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
                0x0040_0101,
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
    fn controller_and_stream_sources_raise_the_hda_msix_vector() {
        let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
        let mut mem = FlatGuestRam::new(RAM_BASE, 0x1000);
        let message_address = 0x0808_2000;
        program_msix_vector(&mut ctrl, 0, message_address, 0x41, true);

        ctrl.rirb_sts = RIRBSTS_RINTFL;
        ctrl.rirb_ctl = RIRBCTL_RINTCTL;
        ctrl.intctl = INTCTL_GIE | INTCTL_CIE;
        let mut messages = Vec::new();
        ctrl.drain_pending_msix_into(true, false, &mut messages);
        assert!(messages.is_empty(), "masked vector must remain pending");
        assert_eq!(
            ctrl.msix_bar_access(u64::from(MSIX_PBA_OFFSET), HdaPciOp::Read { size: 8 }),
            HdaPciResult::ReadValue(1)
        );

        ctrl.msix_bar_access(12, HdaPciOp::Write { size: 4, value: 0 });
        ctrl.drain_pending_msix_into(true, false, &mut messages);
        assert_eq!(
            messages,
            vec![MsixMessage {
                vector: MSIX_CONTROLLER_VECTOR,
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
        ctrl.drain_pending_msix_into(true, false, &mut messages);
        assert!(messages.is_empty());

        ctrl.stream.sts = SDSTS_BCIS;
        ctrl.stream.ctl = SDCTL_IOCE;
        ctrl.intctl = INTCTL_GIE | INTCTL_STREAM0;
        ctrl.drain_pending_msix_into(true, false, &mut messages);
        assert_eq!(
            messages,
            vec![MsixMessage {
                vector: MSIX_CONTROLLER_VECTOR,
                address: message_address,
                data: 0x41,
            }]
        );

        write(&mut ctrl, &mut mem, REG_GCTL, 4, 0);
        assert_eq!(
            ctrl.msix_bar_access(0, HdaPciOp::Read { size: 8 }),
            HdaPciResult::ReadValue(message_address),
            "HDA CRST must not reset PCI MSI-X table programming"
        );
    }
}
