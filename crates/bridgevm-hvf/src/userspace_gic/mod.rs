//! Userspace GICv3 for the full-VM path.
//!
//! A1 evidence (a1-qemu-userspace-gic-control-20260808.md): the same host and
//! guest image boot 10/10 under QEMU-hvf, which emulates the entire GICv3 in
//! userspace and never creates Apple's in-kernel GIC. Our in-kernel-GIC stack
//! measures 14/40 with GIC/vtimer stalls absent from the userspace control.
//! This module is the swap: a self-contained GICv3
//! (distributor + per-CPU redistributors + per-CPU CPU interfaces + GICv2m MSI
//! frame) sized for the full machine (256 INTIDs, SMP affinity routing, SGIs).
//!
//! The run loop drives it with:
//! - `mmio()` for GICD/GICR/MSI-frame data aborts,
//! - `sysreg()` for trapped ICC_* accesses,
//! - `set_spi()` / `send_msi()` from device interrupt delivery,
//! - `set_vtimer_ppi()` from vtimer exits,
//! - `line_asserted()` before every `hv_vcpu_run` to refresh the per-vCPU
//!   IRQ line via `hv_vcpu_set_pending_interrupt`.
//!
//! Cross-vCPU raises are reported as kick lists (vCPU indexes whose IRQ line
//! must be re-evaluated with `hv_vcpus_exit`).

use crate::machine;

pub const GIC_INTID_COUNT: usize = 256;
const SPI_BASE: usize = 32;
/// GICv3 spurious INTID.
pub const SPURIOUS_INTID: u32 = 1023;
/// vtimer PPI INTID (GIC PPI 11).
pub const VTIMER_INTID: u32 = 27;

const GICD_CTLR: u64 = 0x0000;
const GICD_TYPER: u64 = 0x0004;
const GICD_IIDR: u64 = 0x0008;
const GICD_STATUSR: u64 = 0x0010;
const GICD_IGROUPR: u64 = 0x0080;
const GICD_ISENABLER: u64 = 0x0100;
const GICD_ICENABLER: u64 = 0x0180;
const GICD_ISPENDR: u64 = 0x0200;
const GICD_ICPENDR: u64 = 0x0280;
const GICD_ISACTIVER: u64 = 0x0300;
const GICD_ICACTIVER: u64 = 0x0380;
const GICD_IPRIORITYR: u64 = 0x0400;
const GICD_ICFGR: u64 = 0x0c00;
const GICD_IGRPMODR: u64 = 0x0d00;
const GICD_IROUTER: u64 = 0x6000;
const GICD_PIDR2: u64 = 0xffe8;

const GICR_CTLR: u64 = 0x0000;
const GICR_IIDR: u64 = 0x0004;
const GICR_TYPER: u64 = 0x0008;
const GICR_STATUSR: u64 = 0x0010;
const GICR_WAKER: u64 = 0x0014;
const GICR_PROPBASER: u64 = 0x0070;
const GICR_PENDBASER: u64 = 0x0078;
const GICR_PIDR2: u64 = 0xffe8;
const SGI_PAGE: u64 = 0x1_0000;
const GICR_IGROUPR0: u64 = SGI_PAGE + 0x080;
const GICR_ISENABLER0: u64 = SGI_PAGE + 0x100;
const GICR_ICENABLER0: u64 = SGI_PAGE + 0x180;
const GICR_ISPENDR0: u64 = SGI_PAGE + 0x200;
const GICR_ICPENDR0: u64 = SGI_PAGE + 0x280;
const GICR_ISACTIVER0: u64 = SGI_PAGE + 0x300;
const GICR_ICACTIVER0: u64 = SGI_PAGE + 0x380;
const GICR_IPRIORITYR: u64 = SGI_PAGE + 0x400;
const GICR_ICFGR0: u64 = SGI_PAGE + 0xc00;
const GICR_ICFGR1: u64 = SGI_PAGE + 0xc04;
const GICR_IGRPMODR0: u64 = SGI_PAGE + 0xd00;

const GICR_WAKER_PROCESSOR_SLEEP: u32 = 1 << 1;
const GICR_WAKER_CHILDREN_ASLEEP: u32 = 1 << 2;
const GICD_CTLR_ENABLE_G1NS: u32 = 1 << 1;
const GICD_CTLR_ARE_NS: u32 = 1 << 4;
/// Disable Security: single security state (matches QEMU virt secure=off).
const GICD_CTLR_DS: u32 = 1 << 6;
/// GICv3 identification: ArchRev 3 in PIDR2[7:4].
const PIDR2_GICV3: u64 = 0x30;
const IIDR: u64 = 0x4252_564d; // "BRVM"
const IROUTER_IRM: u64 = 1 << 31;

/// GICv2m MSI frame: SET_SPI_NSR register offset inside the frame.
const GICM_TYPER: u64 = 0x008;
const GICM_SET_SPI_NSR: u64 = 0x040;
const GICM_PIDR2: u64 = 0xfe8;

// ICC system registers, encoded as ((op0<<14)|(op1<<11)|(crn<<7)|(crm<<3)|op2).
pub const ICC_PMR_EL1: u16 = 0xc230; // 3,0,4,6,0
pub const ICC_IAR0_EL1: u16 = 0xc640;
pub const ICC_EOIR0_EL1: u16 = 0xc641;
pub const ICC_HPPIR0_EL1: u16 = 0xc642;
pub const ICC_BPR0_EL1: u16 = 0xc643;
pub const ICC_AP0R0_EL1: u16 = 0xc644;
pub const ICC_AP0R1_EL1: u16 = 0xc645;
pub const ICC_AP0R2_EL1: u16 = 0xc646;
pub const ICC_AP0R3_EL1: u16 = 0xc647;
pub const ICC_AP1R0_EL1: u16 = 0xc648;
pub const ICC_AP1R1_EL1: u16 = 0xc649;
pub const ICC_AP1R2_EL1: u16 = 0xc64a;
pub const ICC_AP1R3_EL1: u16 = 0xc64b;
pub const ICC_DIR_EL1: u16 = 0xc659;
pub const ICC_RPR_EL1: u16 = 0xc65b;
pub const ICC_SGI1R_EL1: u16 = 0xc65d;
pub const ICC_ASGI1R_EL1: u16 = 0xc65e;
pub const ICC_SGI0R_EL1: u16 = 0xc65f;
pub const ICC_IAR1_EL1: u16 = 0xc660;
pub const ICC_EOIR1_EL1: u16 = 0xc661;
pub const ICC_HPPIR1_EL1: u16 = 0xc662;
pub const ICC_BPR1_EL1: u16 = 0xc663;
pub const ICC_CTLR_EL1: u16 = 0xc664;
pub const ICC_SRE_EL1: u16 = 0xc665;
pub const ICC_IGRPEN0_EL1: u16 = 0xc666;
pub const ICC_IGRPEN1_EL1: u16 = 0xc667;

const ICC_CTLR_EOIMODE: u64 = 1 << 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsGicMmioResult {
    /// Read data (zero for writes).
    pub value: u64,
    /// IRQ lines may have changed; refresh every vCPU in the mask.
    pub kick_mask: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsGicSysregResult {
    /// Read data (zero for writes).
    pub value: u64,
    pub kick_mask: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ActiveInterrupt {
    intid: u32,
    priority: u8,
    priority_dropped: bool,
}

/// Per-vCPU GICv3 CPU interface (system-register facing).
#[derive(Debug, Clone)]
struct CpuInterface {
    ctlr: u64,
    priority_mask: u8,
    bpr0: u8,
    bpr1: u8,
    group0_enabled: bool,
    group1_enabled: bool,
    active: Vec<ActiveInterrupt>,
    ap0r: [u64; 4],
    ap1r: [u64; 4],
}

impl CpuInterface {
    fn new() -> Self {
        Self {
            ctlr: 0,
            priority_mask: 0,
            bpr0: 0,
            bpr1: 0,
            group0_enabled: false,
            group1_enabled: false,
            active: Vec::new(),
            ap0r: [0; 4],
            ap1r: [0; 4],
        }
    }

    fn running_priority(&self) -> u8 {
        self.active
            .iter()
            .filter(|a| !a.priority_dropped)
            .map(|a| a.priority)
            .min()
            .unwrap_or(0xff)
    }

    fn threshold(&self) -> u8 {
        self.priority_mask.min(self.running_priority())
    }

    fn eoi_mode(&self) -> bool {
        self.ctlr & ICC_CTLR_EOIMODE != 0
    }
}

/// Per-vCPU redistributor: SGIs 0-15 and PPIs 16-31.
#[derive(Debug, Clone)]
struct Redistributor {
    waker: u32,
    propbaser: u64,
    pendbaser: u64,
    group0: u32,
    grpmod0: u32,
    enabled0: u32,
    pending0: u32,
    active0: u32,
    /// Level-sensitive input for PPIs (vtimer): pending while high.
    level0: u32,
    priority: [u8; 32],
    icfgr: [u32; 2],
}

impl Redistributor {
    fn new() -> Self {
        Self {
            // QEMU parity: WAKER is storage-only. EDK2 never touches it (the
            // firmware stalled at BdsDxe when the reset value gated PPIs).
            waker: 0,
            propbaser: 0,
            pendbaser: 0,
            // Single security state (GICD_CTLR.DS=1): everything resets to
            // Group 1 NS. EDK2/Windows never program IGROUPR and only enable
            // ICC_IGRPEN1; a zero reset classified every interrupt as Group 0
            // and nothing ever delivered (usgic-boot2 trace, 2026-08-08).
            group0: 0xffff_ffff,
            grpmod0: 0,
            enabled0: 0,
            pending0: 0,
            active0: 0,
            level0: 0,
            priority: [0; 32],
            icfgr: [0; 2],
        }
    }

    fn effective_pending(&self) -> u32 {
        self.pending0 | self.level0
    }
}

/// Distributor: SPIs 32..GIC_INTID_COUNT with affinity routing.
#[derive(Debug, Clone)]
struct Distributor {
    ctlr: u32,
    group: [u32; GIC_INTID_COUNT / 32],
    grpmod: [u32; GIC_INTID_COUNT / 32],
    enabled: [u32; GIC_INTID_COUNT / 32],
    pending: [u32; GIC_INTID_COUNT / 32],
    active: [u32; GIC_INTID_COUNT / 32],
    /// Level-sensitive input lines (virtio INTx): pending while high.
    level: [u32; GIC_INTID_COUNT / 32],
    priority: [u8; GIC_INTID_COUNT],
    icfgr: [u32; GIC_INTID_COUNT / 16],
    route: [u64; GIC_INTID_COUNT],
}

impl Distributor {
    fn new() -> Self {
        Self {
            ctlr: 0,
            // Group 1 NS at reset -- same DS=1 contract as the redistributor.
            group: [0xffff_ffff; GIC_INTID_COUNT / 32],
            grpmod: [0; GIC_INTID_COUNT / 32],
            enabled: [0; GIC_INTID_COUNT / 32],
            pending: [0; GIC_INTID_COUNT / 32],
            active: [0; GIC_INTID_COUNT / 32],
            level: [0; GIC_INTID_COUNT / 32],
            priority: [0; GIC_INTID_COUNT],
            icfgr: [0; GIC_INTID_COUNT / 16],
            route: [0; GIC_INTID_COUNT],
        }
    }

    fn bit(intid: usize) -> (usize, u32) {
        (intid / 32, 1 << (intid % 32))
    }

    fn effective_pending(&self, reg: usize) -> u32 {
        self.pending[reg] | self.level[reg]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingCandidate {
    intid: u32,
    priority: u8,
}

/// The complete userspace GICv3.
#[derive(Debug, Clone)]
pub struct UserspaceGic {
    num_cpus: usize,
    dist: Distributor,
    redists: Vec<Redistributor>,
    ifaces: Vec<CpuInterface>,
}

impl UserspaceGic {
    pub fn new(num_cpus: usize) -> Self {
        assert!((1..=16).contains(&num_cpus), "cpu count {num_cpus}");
        Self {
            num_cpus,
            dist: Distributor::new(),
            redists: vec![Redistributor::new(); num_cpus],
            ifaces: vec![CpuInterface::new(); num_cpus],
        }
    }

    pub fn num_cpus(&self) -> usize {
        self.num_cpus
    }

    fn all_cpus_mask(&self) -> u64 {
        (1u64 << self.num_cpus) - 1
    }

    fn spi_candidate_for_cpu(&self, cpu: usize, threshold: u8) -> Option<PendingCandidate> {
        if self.dist.ctlr & GICD_CTLR_ENABLE_G1NS == 0 {
            return None;
        }
        (SPI_BASE..GIC_INTID_COUNT)
            .filter_map(|intid| {
                let (reg, bit) = Distributor::bit(intid);
                let group1 = self.dist.group[reg] & bit != 0;
                let enabled = self.dist.enabled[reg] & bit != 0;
                let pending = self.dist.effective_pending(reg) & bit != 0;
                let active = self.dist.active[reg] & bit != 0;
                if !(group1 && enabled && pending && !active) {
                    return None;
                }
                if !self.spi_routes_to_cpu(intid, cpu) {
                    return None;
                }
                let priority = self.dist.priority[intid];
                (priority < threshold).then_some(PendingCandidate {
                    intid: intid as u32,
                    priority,
                })
            })
            .min_by_key(|c| (c.priority, c.intid))
    }

    fn local_candidate_for_cpu(&self, cpu: usize, threshold: u8) -> Option<PendingCandidate> {
        let redist = &self.redists[cpu];
        // WAKER.ProcessorSleep deliberately does NOT gate delivery (QEMU
        // parity): firmware and Windows rely on interrupts flowing without
        // ever programming WAKER.
        (0..32u32)
            .filter_map(|intid| {
                let bit = 1u32 << intid;
                let group1 = redist.group0 & bit != 0;
                let enabled = redist.enabled0 & bit != 0;
                let pending = redist.effective_pending() & bit != 0;
                let active = redist.active0 & bit != 0;
                if !(group1 && enabled && pending && !active) {
                    return None;
                }
                let priority = redist.priority[intid as usize];
                (priority < threshold).then_some(PendingCandidate { intid, priority })
            })
            .min_by_key(|c| (c.priority, c.intid))
    }

    fn highest_candidate(&self, cpu: usize, threshold: u8) -> Option<PendingCandidate> {
        [
            self.local_candidate_for_cpu(cpu, threshold),
            self.spi_candidate_for_cpu(cpu, threshold),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|c| (c.priority, c.intid))
    }

    /// Should this vCPU's IRQ line be asserted right now?
    pub fn line_asserted(&self, cpu: usize) -> bool {
        let iface = &self.ifaces[cpu];
        iface.group1_enabled && self.highest_candidate(cpu, iface.threshold()).is_some()
    }

    fn kick_mask_for(&self, cpus: impl IntoIterator<Item = usize>) -> u64 {
        cpus.into_iter().fold(0u64, |m, c| m | (1u64 << c))
    }

    /// Kick only CPUs whose line level CHANGED. Every `hv_vcpus_exit`
    /// clears the target's exclusive monitor; a kick storm (one per MSI/SGI
    /// while the line was ALREADY asserted) kept guest STXR loops failing
    /// forever -- three vCPUs spinning on one spinlock PC, the fourth
    /// waiting for their IPI acks (r3d-1-074640). An already-asserted line
    /// needs no kick: the pending IRQ is injected at that vCPU's next
    /// entry, which the earlier kick already forced. A falling edge still
    /// kicks so the vCPU re-evaluates the pin down (level SPI deassert).
    fn kick_if_line_changed(&self, cpu: usize, was_asserted: bool) -> u64 {
        if was_asserted != self.line_asserted(cpu) {
            self.kick_mask_for([cpu])
        } else {
            0
        }
    }

    /// Device SPI level (virtio INTx and friends). `intid` is absolute.
    pub fn set_spi(&mut self, intid: u32, level: bool) -> u64 {
        let intid = intid as usize;
        if !(SPI_BASE..GIC_INTID_COUNT).contains(&intid) {
            return 0;
        }
        // Line state BEFORE mutation: the kick decision needs the edge.
        let target = self.route_target(intid);
        let was_line = target.map(|cpu| self.line_asserted(cpu));
        let (reg, bit) = Distributor::bit(intid);
        // Edge-configured SPIs latch into pending on a rising edge; level
        // SPIs track the input. ICFGR bit (2*intid%32+1): 1 = edge.
        let cfg_reg = intid / 16;
        let edge = self.dist.icfgr[cfg_reg] >> ((intid % 16) * 2 + 1) & 1 != 0;
        let was = self.dist.level[reg] & bit != 0;
        if level {
            self.dist.level[reg] |= bit;
            if edge && !was {
                self.dist.pending[reg] |= bit;
            }
        } else {
            self.dist.level[reg] &= !bit;
        }
        if edge {
            // Edge input drops do not clear latched pending.
            self.dist.level[reg] &= !bit;
            if !level {
                return 0;
            }
        }
        match (target, was_line) {
            (Some(cpu), Some(was)) => self.kick_if_line_changed(cpu, was),
            _ => 0,
        }
    }

    /// GICv2m MSI write (`hv_gic_send_msi` replacement): data is the SPI INTID.
    pub fn send_msi(&mut self, address: u64, data: u32) -> u64 {
        if address != machine::GIC_MSI_FRAME.base + GICM_SET_SPI_NSR {
            return 0;
        }
        let intid = data as usize;
        if !(SPI_BASE..GIC_INTID_COUNT).contains(&intid) {
            return 0;
        }
        let target = self.route_target(intid);
        let was = target.map(|cpu| self.line_asserted(cpu));
        // MSIs are always edge: latch pending directly.
        let (reg, bit) = Distributor::bit(intid);
        self.dist.pending[reg] |= bit;
        match (target, was) {
            (Some(cpu), Some(was)) => self.kick_if_line_changed(cpu, was),
            _ => 0,
        }
    }

    /// vtimer PPI for one vCPU. A fire (EXIT_VTIMER) LATCHES pending --
    /// edge semantics -- so the ack still finds it even if the guest's ISR
    /// masks CNTV_CTL.IMASK before reading IAR (Windows does; modelling the
    /// PPI as level tied to CNTV_CTL raced that window and the read
    /// returned spurious, leaving the timer masked forever: usgic-diag3).
    pub fn set_vtimer_ppi(&mut self, cpu: usize, fired: bool) -> u64 {
        let bit = 1u32 << VTIMER_INTID;
        let was_line = self.line_asserted(cpu);
        let redist = &mut self.redists[cpu];
        if fired {
            let was = redist.pending0 & bit != 0;
            redist.pending0 |= bit;
            if !was {
                return self.kick_if_line_changed(cpu, was_line);
            }
        } else {
            redist.pending0 &= !bit;
            redist.level0 &= !bit;
        }
        0
    }

    /// Is the vtimer PPI currently in service (acknowledged, not yet EOI'd)?
    pub fn vtimer_in_service(&self, cpu: usize) -> bool {
        self.redists[cpu].active0 & (1 << VTIMER_INTID) != 0
    }

    /// Is the vtimer PPI enabled by the guest (unmasked at the redistributor)?
    pub fn vtimer_enabled(&self, cpu: usize) -> bool {
        let redist = &self.redists[cpu];
        redist.enabled0 & (1 << VTIMER_INTID) != 0
    }

    fn acknowledge(&mut self, cpu: usize) -> u32 {
        let iface = &self.ifaces[cpu];
        if !iface.group1_enabled {
            return SPURIOUS_INTID;
        }
        let Some(candidate) = self.highest_candidate(cpu, iface.threshold()) else {
            return SPURIOUS_INTID;
        };
        let intid = candidate.intid as usize;
        if intid < 32 {
            let redist = &mut self.redists[cpu];
            let bit = 1u32 << intid;
            redist.pending0 &= !bit;
            redist.active0 |= bit;
        } else {
            let (reg, bit) = Distributor::bit(intid);
            self.dist.pending[reg] &= !bit;
            self.dist.active[reg] |= bit;
        }
        self.ifaces[cpu].active.push(ActiveInterrupt {
            intid: candidate.intid,
            priority: candidate.priority,
            priority_dropped: false,
        });
        candidate.intid
    }

    fn priority_drop(&mut self, cpu: usize, intid: u32) {
        if let Some(active) = self.ifaces[cpu]
            .active
            .iter_mut()
            .rfind(|a| a.intid == intid)
        {
            active.priority_dropped = true;
        }
    }

    fn deactivate(&mut self, cpu: usize, intid: u32) {
        if let Some(position) = self.ifaces[cpu]
            .active
            .iter()
            .rposition(|a| a.intid == intid)
        {
            self.ifaces[cpu].active.remove(position);
        }
        let intid = intid as usize;
        if intid < 32 {
            self.redists[cpu].active0 &= !(1u32 << intid);
        } else if intid < GIC_INTID_COUNT {
            let (reg, bit) = Distributor::bit(intid);
            self.dist.active[reg] &= !bit;
        }
    }

    fn sgi1r_targets(&self, value: u64) -> Vec<usize> {
        if value & (1 << 40) != 0 {
            // IRM: all but self — caller filters self out.
            return (0..self.num_cpus).collect();
        }
        let target_list = value & 0xffff;
        let aff1 = (value >> 16) & 0xff;
        (0..self.num_cpus)
            .filter(|&c| {
                let mpidr = machine::cpu_mpidr(c as u64);
                let cpu_aff1 = (mpidr >> 8) & 0xff;
                let cpu_aff0 = mpidr & 0xff;
                cpu_aff1 == aff1 && cpu_aff0 < 16 && target_list & (1 << cpu_aff0) != 0
            })
            .collect()
    }

    /// Trapped ICC_* system-register access. Returns None if the register is
    /// not a GIC register (caller falls through to its other sysreg handling).
    pub fn sysreg(
        &mut self,
        cpu: usize,
        sys_reg: u16,
        is_read: bool,
        write_value: u64,
    ) -> Option<UsGicSysregResult> {
        let mut kick_mask = 0u64;
        let value = match (is_read, sys_reg) {
            (true, ICC_SRE_EL1) => 0x7,
            (false, ICC_SRE_EL1) => 0,
            (true, ICC_CTLR_EL1) => {
                // PRIbits=7 (8-bit priority), IDbits=0b000 (16-bit INTID range
                // is enough: reads back as 16-bit capable? Windows reads
                // PRIbits/IDbits for formatting; report IDbits=1 (24-bit).
                self.ifaces[cpu].ctlr | (7 << 8) | (1 << 11)
            }
            (false, ICC_CTLR_EL1) => {
                self.ifaces[cpu].ctlr = write_value & ICC_CTLR_EOIMODE;
                0
            }
            (true, ICC_PMR_EL1) => u64::from(self.ifaces[cpu].priority_mask),
            (false, ICC_PMR_EL1) => {
                self.ifaces[cpu].priority_mask = (write_value & 0xff) as u8;
                kick_mask |= 1u64 << cpu;
                0
            }
            (true, ICC_BPR0_EL1) => u64::from(self.ifaces[cpu].bpr0),
            (false, ICC_BPR0_EL1) => {
                self.ifaces[cpu].bpr0 = (write_value & 0x7) as u8;
                0
            }
            (true, ICC_BPR1_EL1) => u64::from(self.ifaces[cpu].bpr1.max(1)),
            (false, ICC_BPR1_EL1) => {
                self.ifaces[cpu].bpr1 = (write_value & 0x7) as u8;
                0
            }
            (true, ICC_RPR_EL1) => u64::from(self.ifaces[cpu].running_priority()),
            (true, ICC_IGRPEN0_EL1) => u64::from(self.ifaces[cpu].group0_enabled),
            (false, ICC_IGRPEN0_EL1) => {
                self.ifaces[cpu].group0_enabled = write_value & 1 != 0;
                0
            }
            (true, ICC_IGRPEN1_EL1) => u64::from(self.ifaces[cpu].group1_enabled),
            (false, ICC_IGRPEN1_EL1) => {
                self.ifaces[cpu].group1_enabled = write_value & 1 != 0;
                kick_mask |= 1u64 << cpu;
                0
            }
            (true, ICC_IAR1_EL1) => u64::from(self.acknowledge(cpu)),
            (true, ICC_IAR0_EL1 | ICC_HPPIR0_EL1) => u64::from(SPURIOUS_INTID),
            (true, ICC_HPPIR1_EL1) => {
                let iface = &self.ifaces[cpu];
                u64::from(
                    self.highest_candidate(cpu, iface.threshold())
                        .map(|c| c.intid)
                        .unwrap_or(SPURIOUS_INTID),
                )
            }
            (false, ICC_EOIR1_EL1) => {
                let intid = (write_value & 0xff_ffff) as u32;
                if intid != SPURIOUS_INTID {
                    self.priority_drop(cpu, intid);
                    if !self.ifaces[cpu].eoi_mode() {
                        self.deactivate(cpu, intid);
                    }
                    kick_mask |= 1u64 << cpu;
                }
                0
            }
            (false, ICC_EOIR0_EL1) => 0,
            (false, ICC_DIR_EL1) => {
                let intid = (write_value & 0xff_ffff) as u32;
                if intid != SPURIOUS_INTID {
                    self.deactivate(cpu, intid);
                    kick_mask |= 1u64 << cpu;
                }
                0
            }
            (false, ICC_SGI1R_EL1) => {
                let intid = ((write_value >> 24) & 0xf) as u32;
                let irm = write_value & (1 << 40) != 0;
                let bit = 1u32 << intid;
                let targets = self.sgi1r_targets(write_value);
                for target in targets {
                    if irm && target == cpu {
                        continue;
                    }
                    let was = self.line_asserted(target);
                    self.redists[target].pending0 |= bit;
                    kick_mask |= self.kick_if_line_changed(target, was);
                }
                0
            }
            (false, ICC_ASGI1R_EL1 | ICC_SGI0R_EL1) => 0,
            (true, ICC_AP0R0_EL1) => self.ifaces[cpu].ap0r[0],
            (true, ICC_AP0R1_EL1) => self.ifaces[cpu].ap0r[1],
            (true, ICC_AP0R2_EL1) => self.ifaces[cpu].ap0r[2],
            (true, ICC_AP0R3_EL1) => self.ifaces[cpu].ap0r[3],
            (true, ICC_AP1R0_EL1) => self.ifaces[cpu].ap1r[0],
            (true, ICC_AP1R1_EL1) => self.ifaces[cpu].ap1r[1],
            (true, ICC_AP1R2_EL1) => self.ifaces[cpu].ap1r[2],
            (true, ICC_AP1R3_EL1) => self.ifaces[cpu].ap1r[3],
            (false, ICC_AP0R0_EL1) => {
                self.ifaces[cpu].ap0r[0] = write_value;
                0
            }
            (false, ICC_AP0R1_EL1) => {
                self.ifaces[cpu].ap0r[1] = write_value;
                0
            }
            (false, ICC_AP0R2_EL1) => {
                self.ifaces[cpu].ap0r[2] = write_value;
                0
            }
            (false, ICC_AP0R3_EL1) => {
                self.ifaces[cpu].ap0r[3] = write_value;
                0
            }
            (false, ICC_AP1R0_EL1) => {
                self.ifaces[cpu].ap1r[0] = write_value;
                0
            }
            (false, ICC_AP1R1_EL1) => {
                self.ifaces[cpu].ap1r[1] = write_value;
                0
            }
            (false, ICC_AP1R2_EL1) => {
                self.ifaces[cpu].ap1r[2] = write_value;
                0
            }
            (false, ICC_AP1R3_EL1) => {
                self.ifaces[cpu].ap1r[3] = write_value;
                0
            }
            _ => return None,
        };
        Some(UsGicSysregResult { value, kick_mask })
    }

    /// One-line diagnostic snapshot for a CPU (stall reports).
    pub fn debug_line(&self, cpu: usize) -> String {
        let iface = &self.ifaces[cpu];
        let redist = &self.redists[cpu];
        let dist_pend: Vec<usize> = (SPI_BASE..GIC_INTID_COUNT)
            .filter(|&i| {
                let (reg, bit) = Distributor::bit(i);
                self.dist.effective_pending(reg) & bit != 0
            })
            .collect();
        format!(
            "grp1={} pmr={:#x} rpr={:#x} active={:?} redist(en={:#x} pend={:#x} lvl={:#x} act={:#x}) dist(ctlr={:#x} pend={:?}) line={}",
            iface.group1_enabled,
            iface.priority_mask,
            iface.running_priority(),
            iface.active.iter().map(|a| a.intid).collect::<Vec<_>>(),
            redist.enabled0,
            redist.pending0,
            redist.level0,
            redist.active0,
            self.dist.ctlr,
            dist_pend,
            self.line_asserted(cpu),
        )
    }

    /// Does this IPA belong to the userspace GIC (GICD/GICR/MSI frame)?
    pub fn owns(ipa: u64) -> bool {
        machine::GIC_DIST.contains(ipa)
            || machine::GIC_REDIST.contains(ipa)
            || machine::GIC_MSI_FRAME.contains(ipa)
    }

    /// MMIO access from a data abort. `write` carries the store value.
    pub fn mmio(&mut self, ipa: u64, width: u8, write: Option<u64>) -> UsGicMmioResult {
        if machine::GIC_DIST.contains(ipa) {
            return self.dist_mmio(ipa - machine::GIC_DIST.base, width, write);
        }
        if machine::GIC_REDIST.contains(ipa) {
            let offset = ipa - machine::GIC_REDIST.base;
            let cpu = (offset / machine::GICV3_REDIST_STRIDE) as usize;
            let reg = offset % machine::GICV3_REDIST_STRIDE;
            if cpu < self.num_cpus {
                return self.redist_mmio(cpu, reg, width, write);
            }
            return UsGicMmioResult {
                value: 0,
                kick_mask: 0,
            };
        }
        if machine::GIC_MSI_FRAME.contains(ipa) {
            return self.msi_frame_mmio(ipa - machine::GIC_MSI_FRAME.base, write);
        }
        UsGicMmioResult {
            value: 0,
            kick_mask: 0,
        }
    }
}

mod mmio_regs;
mod routing;

#[cfg(test)]
#[path = "routing_tests.rs"]
mod routing_tests;

#[cfg(test)]
#[path = "userspace_gic_tests.rs"]
mod userspace_gic_tests;
