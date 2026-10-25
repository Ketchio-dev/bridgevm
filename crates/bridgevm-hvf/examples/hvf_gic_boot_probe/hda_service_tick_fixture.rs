//! A machine with HDA and MSI, and a guest that refills one tagged period per
//! MSI it takes, for the service tick's tests. The stream is Windows' as T17
//! r92 recorded it: 48 kHz stereo 16-bit, two 10 ms periods, IOC on both.

use super::*;
use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::hda::{HdaPcmSink, REG_GCTL, REG_INTCTL, REG_SD_BDPL, REG_SD_CBL};
use bridgevm_hvf::hda::{REG_SD_CTL, REG_SD_FMT, REG_SD_LVI, REG_SD_STS};
use bridgevm_hvf::pcie;
use bridgevm_hvf::platform_virt::VirtPlatformDeviceConfig;
use bridgevm_hvf::platform_virt::{FlatGuestRam, MmioOp, MmioOutcome, VirtPlatformConfig};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicU64;

pub(super) const BAR: u64 = machine::PCIE_MMIO_32.base + 0x70_000;
pub(super) const HDA_MSI: MsixMessage = MsixMessage {
    vector: 0,
    address: 0x0000_0001_0808_4000,
    data: 0x61,
};
const BDL: u64 = machine::RAM_BASE + 0x1000;
const PCM: u64 = machine::RAM_BASE + 0x2000;
/// One period: 10 ms of the stream.
pub(super) const PERIOD: Duration = Duration::from_millis(10);
const PERIOD_BYTES: usize = 1920;
pub(super) const BYTES_PER_MS: u64 = 192;
pub(super) const RUN_IOCE: u64 = (1 << 1) | (1 << 2);
const BCIS: u64 = 1 << 2;

/// Guest RAM holding the two-entry BDL and both periods, tagged laps 1 and 2.
pub(super) fn guest_ram() -> FlatGuestRam {
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1_0000);
    for period in 0..2 {
        let mut descriptor = [0u8; 16];
        let address = PCM + period * PERIOD_BYTES as u64;
        descriptor[..8].copy_from_slice(&address.to_le_bytes());
        descriptor[8..12].copy_from_slice(&(PERIOD_BYTES as u32).to_le_bytes());
        descriptor[12..16].copy_from_slice(&1u32.to_le_bytes());
        assert!(mem.write_bytes(BDL + period * 16, &descriptor));
        tag(&mut mem, period as usize, period as u32 + 1);
    }
    mem
}

/// Fill `period` with lap `lap`'s bytes: audible, and unlike any other lap's.
fn tag(mem: &mut FlatGuestRam, period: usize, lap: u32) {
    let bytes: Vec<u8> = lap
        .to_le_bytes()
        .into_iter()
        .cycle()
        .take(PERIOD_BYTES)
        .collect();
    assert!(mem.write_bytes(PCM + (period * PERIOD_BYTES) as u64, &bytes));
}

/// Counts the PCM bytes the DMA hands to the host.
struct CountingSink(Arc<AtomicU64>);

impl HdaPcmSink for CountingSink {
    fn write_pcm(&mut self, samples: &[u8], _rate: u32, _channels: u8, _bits: u8) {
        self.0.fetch_add(samples.len() as u64, SeqCst);
    }
}

/// HDA with MSI enabled and the stream programmed, not running, and the
/// counter of PCM bytes its DMA plays.
pub(super) fn hda_platform() -> (Arc<Mutex<VirtPlatform>>, Arc<AtomicU64>) {
    let mut platform = VirtPlatform::new_with_config(VirtPlatformConfig {
        fdt: VirtFdtConfig::default(),
        devices: VirtPlatformDeviceConfig {
            hda_present: true,
            ..VirtPlatformDeviceConfig::default()
        },
    });
    let played = Arc::new(AtomicU64::new(0));
    platform.set_hda_pcm_sink(Some(Box::new(CountingSink(Arc::clone(&played)))));
    program(&mut platform);
    (Arc::new(Mutex::new(platform)), played)
}

/// What the guest's firmware and driver program before RUN, after any reset.
pub(super) fn program(platform: &mut VirtPlatform) {
    let (msi, mut mem) = (u16::from(pcie::HDA_MSI_CAP_OFFSET), guest_ram());
    let (_, device, function) = pcie::HDA_BDF;
    let function = (u64::from(device) << 15) | (u64::from(function) << 12);
    let cfg = |reg: u16| machine::PCIE_ECAM.base + function + u64::from(reg);
    let command = u64::from(pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER);
    for (address, size, value) in [
        (cfg(pcie::REG_BAR0), 4, BAR),
        (cfg(pcie::REG_COMMAND_STATUS), 2, command),
        (cfg(msi + 4), 4, HDA_MSI.address as u32 as u64),
        (cfg(msi + 8), 4, HDA_MSI.address >> 32),
        (cfg(msi + 12), 2, u64::from(HDA_MSI.data)),
        (cfg(msi + 2), 2, 1),
        (BAR + REG_GCTL, 4, 1),
        (BAR + REG_INTCTL, 4, (1 << 31) | 1),
        (BAR + REG_SD_BDPL, 4, BDL),
        (BAR + REG_SD_CBL, 4, 2 * PERIOD_BYTES as u64),
        (BAR + REG_SD_LVI, 2, 1),
        (BAR + REG_SD_FMT, 2, 0x0011),
    ] {
        mmio_write(platform, &mut mem, address, size, value);
    }
}

/// A vCPU's SD0CTL write: RUN with IOC interrupts enabled, or a stop.
pub(super) fn run_stream(platform: &Mutex<VirtPlatform>, run: bool) {
    let mut platform = platform.lock().unwrap();
    let value = if run { RUN_IOCE } else { 0 };
    mmio_write(&mut platform, &mut guest_ram(), BAR + REG_SD_CTL, 1, value);
}

/// A vCPU's acknowledgement of the stream's completion interrupt.
pub(super) fn ack(platform: &mut VirtPlatform, mem: &mut FlatGuestRam) {
    mmio_write(platform, mem, BAR + REG_SD_STS, 1, BCIS);
}

pub(super) fn mmio_write(
    platform: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    address: u64,
    size: u8,
    value: u64,
) {
    let op = MmioOp::Write { size, value };
    assert_eq!(platform.on_mmio(address, op, mem), MmioOutcome::WriteAck);
}

/// The guest: it takes HDA's MSIs, acknowledges BCIS and refills one period
/// per MSI (it reads no position, as T17 r92's guest did not), and it is the
/// tick's port, so a tick's MSI reaches its handler at the tick's time.
pub(super) struct Guest {
    pub(super) platform: Arc<Mutex<VirtPlatform>>,
    pub(super) mem: FlatGuestRam,
    pub(super) played: Arc<AtomicU64>,
    /// When each MSI reached the handler.
    pub(super) msis: Vec<Instant>,
    lap: u32,
    next_refill: usize,
    clock: Instant,
}

impl Guest {
    pub(super) fn new(start: Instant) -> Self {
        let (platform, played) = hda_platform();
        Self {
            platform,
            mem: guest_ram(),
            played,
            msis: Vec::new(),
            lap: 2,
            next_refill: 0,
            clock: start,
        }
    }

    pub(super) fn write(&mut self, register: u64, size: u8, value: u64) {
        let mut platform = self.platform.lock().unwrap();
        mmio_write(&mut platform, &mut self.mem, BAR + register, size, value);
    }

    /// A vCPU exit's drain at `now`, as interrupt_delivery/prepare.rs runs it
    /// with that host time: HDA polls, and the MSIs it queued are delivered.
    pub(super) fn exit_poll(&mut self, now: Instant) {
        let messages = {
            let mut platform = self.platform.lock().unwrap();
            platform.set_host_now(now);
            platform.poll_hda(&mut self.mem);
            platform.take_pending_msix()
        };
        for message in messages {
            self.interrupt(message, now);
        }
    }

    fn interrupt(&mut self, message: MsixMessage, now: Instant) {
        assert_eq!(message, HDA_MSI);
        self.msis.push(now);
        ack(&mut self.platform.lock().unwrap(), &mut self.mem);
        self.lap += 1;
        tag(&mut self.mem, self.next_refill, self.lap);
        self.next_refill ^= 1;
    }

    /// The `hda poll:` and `hda repeat:` fields.
    pub(super) fn records(&self) -> BTreeMap<String, u64> {
        let records = self.platform.lock().unwrap().hda_diagnostic_records();
        let fields = records
            .iter()
            .filter(|record| record.starts_with("hda poll:") || record.starts_with("hda repeat:"))
            .flat_map(|record| record.split(' ').skip(2));
        let field = |pair: &str| {
            let (name, value) = pair.split_once('=').unwrap();
            (name.to_string(), value.parse().unwrap())
        };
        fields.map(field).collect()
    }
}

impl TickPort for Guest {
    fn service(&mut self, now: Instant) -> Service {
        self.clock = now;
        service_once(&self.platform, &mut self.mem, now)
    }

    fn send(&mut self, message: MsixMessage) -> HvReturn {
        assert!(self.platform.try_lock().is_ok(), "sent under the lock");
        self.interrupt(message, self.clock);
        0
    }
}
