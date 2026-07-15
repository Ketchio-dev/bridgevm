//! Versioned VM checkpoint serialization for the live HVF probe.
//!
//! A checkpoint contains five length-delimited sections:
//! - META: guest RAM length and sparse-RAM chunk size
//! - RAM0: non-zero 16 KiB RAM chunks
//! - VCPU: architectural vCPU register bundles
//! - GIC0: Apple's opaque, versioned hv_gic state
//! - DEV0: platform/device state supplied by VirtPlatform
//!
//! All integers in the on-disk format are little-endian.

use std::ffi::c_void;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

pub type HvVcpu = u64;

pub const CHECKPOINT_MAGIC: [u8; 8] = *b"BVMCKP03";
pub const CHECKPOINT_VERSION: u32 = 3;
pub const SPARSE_RAM_CHUNK_SIZE: usize = 16 * 1024;

const SECTION_META: [u8; 4] = *b"META";
const SECTION_RAM: [u8; 4] = *b"RAM0";
const SECTION_VCPU: [u8; 4] = *b"VCPU";
const SECTION_GIC: [u8; 4] = *b"GIC0";
const SECTION_DEVICE: [u8; 4] = *b"DEV0";
const SECTION_COUNT: u32 = 5;
const MAX_SECTION_BYTES: u64 = 128 * 1024 * 1024 * 1024;

const HV_SUCCESS: i32 = 0;
const HV_REG_X0: u32 = 0;
const HV_REG_PC: u32 = 31;
const HV_REG_FPCR: u32 = 32;
const HV_REG_FPSR: u32 = 33;
const HV_REG_CPSR: u32 = 34;
const HV_SIMD_FP_REG_Q0: u32 = 0;

const SYS_REGS: &[u16] = &[
    0xc005, // MPIDR_EL1
    0xc080, // SCTLR_EL1
    0xc082, // CPACR_EL1
    0xc100, // TTBR0_EL1
    0xc101, // TTBR1_EL1
    0xc102, // TCR_EL1
    0xc108, // APIAKEYLO_EL1
    0xc109, // APIAKEYHI_EL1
    0xc10a, // APIBKEYLO_EL1
    0xc10b, // APIBKEYHI_EL1
    0xc110, // APDAKEYLO_EL1
    0xc111, // APDAKEYHI_EL1
    0xc112, // APDBKEYLO_EL1
    0xc113, // APDBKEYHI_EL1
    0xc118, // APGAKEYLO_EL1
    0xc119, // APGAKEYHI_EL1
    0xc200, // SPSR_EL1
    0xc201, // ELR_EL1
    0xc208, // SP_EL0
    0xc288, // AFSR0_EL1
    0xc289, // AFSR1_EL1
    0xc290, // ESR_EL1
    0xc300, // FAR_EL1
    0xc3a0, // PAR_EL1
    0xc510, // MAIR_EL1
    0xc518, // AMAIR_EL1
    0xc600, // VBAR_EL1
    0xc681, // CONTEXTIDR_EL1
    0xc684, // TPIDR_EL1
    0xc708, // CNTKCTL_EL1
    0xd000, // CSSELR_EL1
    0xde82, // TPIDR_EL0
    0xde83, // TPIDRRO_EL0
    0xdf11, // CNTP_CTL_EL0
    0xdf12, // CNTP_CVAL_EL0
    0xdf19, // CNTV_CTL_EL0
    0xdf1a, // CNTV_CVAL_EL0
    0xe208, // SP_EL1
];

const GIC_ICC_REGS: &[u16] = &[
    0xc230, // ICC_PMR_EL1
    0xc643, // ICC_BPR0_EL1
    0xc644, // ICC_AP0R0_EL1
    0xc648, // ICC_AP1R0_EL1
    0xc65b, // ICC_RPR_EL1
    0xc663, // ICC_BPR1_EL1
    0xc664, // ICC_CTLR_EL1
    0xc665, // ICC_SRE_EL1
    0xc666, // ICC_IGRPEN0_EL1
    0xc667, // ICC_IGRPEN1_EL1
];

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
/// 16-byte Q-register image. Kept out of the FFI surface: passing SIMD
/// vectors through `extern "C"` requires the unstable `simd_ffi` feature, so
/// the getter goes through a byte pointer cast and the setter through an
/// inline-asm trampoline that places the value in `v0` per AAPCS64.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct HvSimdValue {
    bytes: [u8; 16],
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[link(name = "Hypervisor", kind = "framework")]
extern "C" {
    fn hv_vcpus_exit(vcpus: *const HvVcpu, vcpu_count: u32) -> i32;

    fn hv_vcpu_get_reg(vcpu: HvVcpu, reg: u32, value: *mut u64) -> i32;
    fn hv_vcpu_set_reg(vcpu: HvVcpu, reg: u32, value: u64) -> i32;
    fn hv_vcpu_get_sys_reg(vcpu: HvVcpu, reg: u16, value: *mut u64) -> i32;
    fn hv_vcpu_set_sys_reg(vcpu: HvVcpu, reg: u16, value: u64) -> i32;
    fn hv_vcpu_get_simd_fp_reg(
        vcpu: HvVcpu,
        reg: u32,
        value: *mut HvSimdValue,
    ) -> i32;

    fn hv_vcpu_get_vtimer_mask(vcpu: HvVcpu, masked: *mut bool) -> i32;
    fn hv_vcpu_set_vtimer_mask(vcpu: HvVcpu, masked: bool) -> i32;
    fn hv_vcpu_get_vtimer_offset(vcpu: HvVcpu, offset: *mut u64) -> i32;
    fn hv_vcpu_set_vtimer_offset(vcpu: HvVcpu, offset: u64) -> i32;

    fn hv_gic_get_icc_reg(vcpu: HvVcpu, reg: u16, value: *mut u64) -> i32;
    fn hv_gic_set_icc_reg(vcpu: HvVcpu, reg: u16, value: u64) -> i32;

    fn hv_gic_state_create() -> *mut c_void;
    fn hv_gic_state_get_size(state: *mut c_void, size: *mut usize) -> i32;
    fn hv_gic_state_get_data(state: *mut c_void, data: *mut c_void) -> i32;
    fn hv_gic_set_state(data: *const c_void, size: usize) -> i32;
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
extern "C" {
    fn os_release(object: *mut c_void);
}

/// `hv_vcpu_set_simd_fp_reg` takes `hv_simd_fp_uchar16_t` BY VALUE (in `v0`).
/// Stable Rust cannot express that ABI in an extern decl, so load `q0`
/// manually and branch to the symbol.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe fn hv_vcpu_set_simd_fp_reg_by_value(vcpu: HvVcpu, reg: u32, value: &HvSimdValue) -> i32 {
    let ret: i32;
    std::arch::asm!(
        "ldr q0, [{ptr}]",
        "bl {func}",
        ptr = in(reg) value.bytes.as_ptr(),
        func = sym hv_vcpu_set_simd_fp_reg_extern,
        in("x0") vcpu,
        in("w1") reg,
        lateout("w0") ret,
        out("v0") _,
        clobber_abi("C"),
    );
    ret
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
extern "C" {
    #[link_name = "hv_vcpu_set_simd_fp_reg"]
    fn hv_vcpu_set_simd_fp_reg_extern();
}


#[derive(Debug, Clone)]
pub struct SparseRamChunk {
    pub offset: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct VcpuRegisterBundle {
    pub x: [u64; 31],
    pub pc: u64,
    pub fpcr: u64,
    pub fpsr: u64,
    pub cpsr: u64,
    pub sys_regs: Vec<(u16, u64)>,
    pub simd: [[u8; 16]; 32],
    pub gic_icc_regs: Vec<(u16, u64)>,
    pub vtimer_offset: u64,
    pub vtimer_masked: bool,
}

#[derive(Debug, Clone)]
pub struct VmCheckpoint {
    pub ram_len: u64,
    pub ram_chunks: Vec<SparseRamChunk>,
    pub vcpus: Vec<VcpuRegisterBundle>,
    pub gic_state: Vec<u8>,
    pub device_state: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct StateWriter {
    bytes: Vec<u8>,
}

impl StateWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub fn write_bool(&mut self, value: bool) {
        self.write_u8(u8::from(value));
    }

    pub fn write_u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn write_u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn write_u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn write_blob(&mut self, value: &[u8]) {
        self.write_u64(value.len() as u64);
        self.bytes.extend_from_slice(value);
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
}

#[derive(Debug)]
pub struct StateReader<'a> {
    cursor: Cursor<'a>,
}

impl<'a> StateReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(bytes),
        }
    }

    pub fn read_u8(&mut self) -> u8 {
        self.cursor.u8().expect("truncated device snapshot")
    }

    pub fn read_bool(&mut self) -> bool {
        match self.read_u8() {
            0 => false,
            1 => true,
            value => panic!("invalid snapshot boolean {value}"),
        }
    }

    pub fn read_u16(&mut self) -> u16 {
        self.cursor.u16().expect("truncated device snapshot")
    }

    pub fn read_u32(&mut self) -> u32 {
        self.cursor.u32().expect("truncated device snapshot")
    }

    pub fn read_u64(&mut self) -> u64 {
        self.cursor.u64().expect("truncated device snapshot")
    }

    pub fn read_blob(&mut self) -> Vec<u8> {
        self.cursor.blob().expect("truncated device snapshot")
    }

    pub fn finish(self) {
        assert!(
            self.cursor.is_finished(),
            "trailing bytes in device snapshot"
        );
    }
}

impl VmCheckpoint {
    pub fn capture(
        vcpus: &[HvVcpu],
        ram: &[u8],
        device_state: Vec<u8>,
    ) -> io::Result<Self> {
        if vcpus.is_empty() {
            return Err(invalid("checkpoint requires at least one vCPU"));
        }

        let mut vcpu_states = Vec::with_capacity(vcpus.len());
        for &vcpu in vcpus {
            vcpu_states.push(VcpuRegisterBundle::capture(vcpu)?);
        }

        Ok(Self {
            ram_len: ram.len() as u64,
            ram_chunks: sparse_ram_chunks(ram),
            vcpus: vcpu_states,
            gic_state: capture_gic_state()?,
            device_state,
        })
    }

    pub fn restore_hvf(&self, vcpus: &[HvVcpu], ram: &mut [u8]) -> io::Result<()> {
        if self.vcpus.len() != vcpus.len() {
            return Err(invalid(format!(
                "checkpoint has {} vCPUs but VM has {}",
                self.vcpus.len(),
                vcpus.len()
            )));
        }
        if self.ram_len != ram.len() as u64 {
            return Err(invalid(format!(
                "checkpoint RAM is {} bytes but VM RAM is {} bytes",
                self.ram_len,
                ram.len()
            )));
        }

        restore_sparse_ram(ram, &self.ram_chunks)?;
        restore_gic_state(&self.gic_state)?;

        for (&vcpu, state) in vcpus.iter().zip(&self.vcpus) {
            state.restore(vcpu)?;
        }
        Ok(())
    }

    pub fn write_to_path(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(&CHECKPOINT_MAGIC)?;
        file.write_all(&CHECKPOINT_VERSION.to_le_bytes())?;
        file.write_all(&SECTION_COUNT.to_le_bytes())?;

        let mut meta = StateWriter::new();
        meta.write_u64(self.ram_len);
        meta.write_u32(SPARSE_RAM_CHUNK_SIZE as u32);
        meta.write_u32(0);

        write_section(&mut file, SECTION_META, &meta.into_inner())?;
        write_section(&mut file, SECTION_RAM, &encode_ram_chunks(&self.ram_chunks))?;
        write_section(&mut file, SECTION_VCPU, &encode_vcpus(&self.vcpus))?;
        write_section(&mut file, SECTION_GIC, &self.gic_state)?;
        write_section(&mut file, SECTION_DEVICE, &self.device_state)?;
        file.flush()
    }

    pub fn read_from_path(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut header = [0u8; 16];
        file.read_exact(&mut header)?;

        if header[..8] != CHECKPOINT_MAGIC {
            return Err(invalid("checkpoint magic mismatch"));
        }
        let version = u32::from_le_bytes(header[8..12].try_into().unwrap());
        if version != CHECKPOINT_VERSION {
            return Err(invalid(format!(
                "unsupported checkpoint version {version}"
            )));
        }
        let section_count = u32::from_le_bytes(header[12..16].try_into().unwrap());
        if section_count != SECTION_COUNT {
            return Err(invalid(format!(
                "checkpoint has {section_count} sections, expected {SECTION_COUNT}"
            )));
        }

        let mut meta = None;
        let mut ram = None;
        let mut vcpu = None;
        let mut gic = None;
        let mut device = None;

        for _ in 0..section_count {
            let (tag, payload) = read_section(&mut file)?;
            let slot = match tag {
                SECTION_META => &mut meta,
                SECTION_RAM => &mut ram,
                SECTION_VCPU => &mut vcpu,
                SECTION_GIC => &mut gic,
                SECTION_DEVICE => &mut device,
                _ => return Err(invalid("unknown checkpoint section")),
            };
            if slot.replace(payload).is_some() {
                return Err(invalid("duplicate checkpoint section"));
            }
        }

        let mut tail = [0u8; 1];
        if file.read(&mut tail)? != 0 {
            return Err(invalid("trailing bytes after checkpoint sections"));
        }

        let mut meta = Cursor::new(meta.as_deref().ok_or_else(|| invalid("missing META"))?);
        let ram_len = meta.u64()?;
        let chunk_size = meta.u32()?;
        let reserved = meta.u32()?;
        if chunk_size as usize != SPARSE_RAM_CHUNK_SIZE || reserved != 0 || !meta.is_finished() {
            return Err(invalid("invalid META section"));
        }

        Ok(Self {
            ram_len,
            ram_chunks: decode_ram_chunks(
                ram.as_deref().ok_or_else(|| invalid("missing RAM0"))?,
                ram_len,
            )?,
            vcpus: decode_vcpus(
                vcpu.as_deref().ok_or_else(|| invalid("missing VCPU"))?,
            )?,
            gic_state: gic.ok_or_else(|| invalid("missing GIC0"))?,
            device_state: device.ok_or_else(|| invalid("missing DEV0"))?,
        })
    }
}

impl VcpuRegisterBundle {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    pub fn capture(vcpu: HvVcpu) -> io::Result<Self> {
        let mut state = Self {
            x: [0; 31],
            pc: 0,
            fpcr: 0,
            fpsr: 0,
            cpsr: 0,
            sys_regs: Vec::with_capacity(SYS_REGS.len()),
            simd: [[0; 16]; 32],
            gic_icc_regs: Vec::with_capacity(GIC_ICC_REGS.len()),
            vtimer_offset: 0,
            vtimer_masked: false,
        };

        for (index, value) in state.x.iter_mut().enumerate() {
            hv(
                unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0 + index as u32, value) },
                "hv_vcpu_get_reg(Xn)",
            )?;
        }
        get_reg(vcpu, HV_REG_PC, &mut state.pc)?;
        get_reg(vcpu, HV_REG_FPCR, &mut state.fpcr)?;
        get_reg(vcpu, HV_REG_FPSR, &mut state.fpsr)?;
        get_reg(vcpu, HV_REG_CPSR, &mut state.cpsr)?;

        for &reg in SYS_REGS {
            let mut value = 0;
            hv(
                unsafe { hv_vcpu_get_sys_reg(vcpu, reg, &mut value) },
                "hv_vcpu_get_sys_reg",
            )?;
            state.sys_regs.push((reg, value));
        }

        for (index, bytes) in state.simd.iter_mut().enumerate() {
            let mut value = HvSimdValue { bytes: [0u8; 16] };
            hv(
                unsafe {
                    hv_vcpu_get_simd_fp_reg(
                        vcpu,
                        HV_SIMD_FP_REG_Q0 + index as u32,
                        &mut value,
                    )
                },
                "hv_vcpu_get_simd_fp_reg",
            )?;
            *bytes = value.bytes;
        }

        for &reg in GIC_ICC_REGS {
            let mut value = 0;
            hv(
                unsafe { hv_gic_get_icc_reg(vcpu, reg, &mut value) },
                "hv_gic_get_icc_reg",
            )?;
            state.gic_icc_regs.push((reg, value));
        }

        hv(
            unsafe { hv_vcpu_get_vtimer_offset(vcpu, &mut state.vtimer_offset) },
            "hv_vcpu_get_vtimer_offset",
        )?;
        hv(
            unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut state.vtimer_masked) },
            "hv_vcpu_get_vtimer_mask",
        )?;
        Ok(state)
    }

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    pub fn capture(_vcpu: HvVcpu) -> io::Result<Self> {
        Err(unsupported())
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    pub fn restore(&self, vcpu: HvVcpu) -> io::Result<()> {
        hv(
            unsafe { hv_vcpu_set_vtimer_offset(vcpu, self.vtimer_offset) },
            "hv_vcpu_set_vtimer_offset",
        )?;

        for &(reg, value) in &self.sys_regs {
            hv(
                unsafe { hv_vcpu_set_sys_reg(vcpu, reg, value) },
                "hv_vcpu_set_sys_reg",
            )?;
        }

        for &(reg, value) in &self.gic_icc_regs {
            hv(
                unsafe { hv_gic_set_icc_reg(vcpu, reg, value) },
                "hv_gic_set_icc_reg",
            )?;
        }

        for (index, bytes) in self.simd.iter().enumerate() {
            let value = HvSimdValue { bytes: *bytes };
            hv(
                unsafe {
                    hv_vcpu_set_simd_fp_reg_by_value(
                        vcpu,
                        HV_SIMD_FP_REG_Q0 + index as u32,
                        &value,
                    )
                },
                "hv_vcpu_set_simd_fp_reg",
            )?;
        }

        for (index, value) in self.x.iter().copied().enumerate() {
            set_reg(vcpu, HV_REG_X0 + index as u32, value)?;
        }
        set_reg(vcpu, HV_REG_FPCR, self.fpcr)?;
        set_reg(vcpu, HV_REG_FPSR, self.fpsr)?;
        set_reg(vcpu, HV_REG_CPSR, self.cpsr)?;
        set_reg(vcpu, HV_REG_PC, self.pc)?;

        hv(
            unsafe { hv_vcpu_set_vtimer_mask(vcpu, self.vtimer_masked) },
            "hv_vcpu_set_vtimer_mask",
        )
    }

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    pub fn restore(&self, _vcpu: HvVcpu) -> io::Result<()> {
        Err(unsupported())
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn request_vcpu_exit(vcpus: &[HvVcpu]) -> io::Result<()> {
    let count = u32::try_from(vcpus.len())
        .map_err(|_| invalid("vCPU count does not fit Hypervisor API"))?;
    hv(
        unsafe { hv_vcpus_exit(vcpus.as_ptr(), count) },
        "hv_vcpus_exit",
    )
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub fn request_vcpu_exit(_vcpus: &[HvVcpu]) -> io::Result<()> {
    Err(unsupported())
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn capture_gic_state() -> io::Result<Vec<u8>> {
    struct GicState(*mut c_void);

    impl Drop for GicState {
        fn drop(&mut self) {
            unsafe { os_release(self.0) };
        }
    }

    let state = GicState(unsafe { hv_gic_state_create() });
    if state.0.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "hv_gic_state_create returned null; the VM must be fully stopped",
        ));
    }

    let mut size = 0usize;
    hv(
        unsafe { hv_gic_state_get_size(state.0, &mut size) },
        "hv_gic_state_get_size",
    )?;
    let mut data = vec![0u8; size];
    hv(
        unsafe { hv_gic_state_get_data(state.0, data.as_mut_ptr().cast()) },
        "hv_gic_state_get_data",
    )?;
    Ok(data)
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn capture_gic_state() -> io::Result<Vec<u8>> {
    Err(unsupported())
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn restore_gic_state(data: &[u8]) -> io::Result<()> {
    if data.is_empty() {
        return Err(invalid("empty GIC state"));
    }
    hv(
        unsafe { hv_gic_set_state(data.as_ptr().cast(), data.len()) },
        "hv_gic_set_state",
    )
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn restore_gic_state(_data: &[u8]) -> io::Result<()> {
    Err(unsupported())
}

fn sparse_ram_chunks(ram: &[u8]) -> Vec<SparseRamChunk> {
    ram.chunks(SPARSE_RAM_CHUNK_SIZE)
        .enumerate()
        .filter(|(_, chunk)| chunk.iter().any(|byte| *byte != 0))
        .map(|(index, chunk)| SparseRamChunk {
            offset: (index * SPARSE_RAM_CHUNK_SIZE) as u64,
            bytes: chunk.to_vec(),
        })
        .collect()
}

fn restore_sparse_ram(ram: &mut [u8], chunks: &[SparseRamChunk]) -> io::Result<()> {
    ram.fill(0);
    for chunk in chunks {
        let start = usize::try_from(chunk.offset)
            .map_err(|_| invalid("RAM chunk offset does not fit usize"))?;
        let end = start
            .checked_add(chunk.bytes.len())
            .ok_or_else(|| invalid("RAM chunk range overflow"))?;
        if end > ram.len() || chunk.bytes.len() > SPARSE_RAM_CHUNK_SIZE {
            return Err(invalid("RAM chunk is outside guest RAM"));
        }
        ram[start..end].copy_from_slice(&chunk.bytes);
    }
    Ok(())
}

fn encode_ram_chunks(chunks: &[SparseRamChunk]) -> Vec<u8> {
    let mut out = StateWriter::new();
    out.write_u32(chunks.len() as u32);
    out.write_u32(0);
    for chunk in chunks {
        out.write_u64(chunk.offset);
        out.write_blob(&chunk.bytes);
    }
    out.into_inner()
}

fn decode_ram_chunks(data: &[u8], ram_len: u64) -> io::Result<Vec<SparseRamChunk>> {
    let mut cursor = Cursor::new(data);
    let count = cursor.u32()? as usize;
    if cursor.u32()? != 0 {
        return Err(invalid("invalid RAM0 reserved field"));
    }

    let mut chunks = Vec::with_capacity(count);
    let mut previous_end = 0u64;
    for _ in 0..count {
        let offset = cursor.u64()?;
        let bytes = cursor.blob()?;
        let end = offset
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| invalid("RAM chunk range overflow"))?;
        if offset < previous_end
            || end > ram_len
            || bytes.is_empty()
            || bytes.len() > SPARSE_RAM_CHUNK_SIZE
            || offset % SPARSE_RAM_CHUNK_SIZE as u64 != 0
        {
            return Err(invalid("invalid or overlapping sparse RAM chunk"));
        }
        previous_end = end;
        chunks.push(SparseRamChunk { offset, bytes });
    }
    if !cursor.is_finished() {
        return Err(invalid("trailing bytes in RAM0"));
    }
    Ok(chunks)
}

fn encode_vcpus(vcpus: &[VcpuRegisterBundle]) -> Vec<u8> {
    let mut out = StateWriter::new();
    out.write_u32(vcpus.len() as u32);
    for state in vcpus {
        for value in state.x {
            out.write_u64(value);
        }
        out.write_u64(state.pc);
        out.write_u64(state.fpcr);
        out.write_u64(state.fpsr);
        out.write_u64(state.cpsr);

        out.write_u32(state.sys_regs.len() as u32);
        for &(reg, value) in &state.sys_regs {
            out.write_u16(reg);
            out.write_u16(0);
            out.write_u32(0);
            out.write_u64(value);
        }

        for value in &state.simd {
            out.bytes.extend_from_slice(value);
        }

        out.write_u32(state.gic_icc_regs.len() as u32);
        for &(reg, value) in &state.gic_icc_regs {
            out.write_u16(reg);
            out.write_u16(0);
            out.write_u32(0);
            out.write_u64(value);
        }

        out.write_u64(state.vtimer_offset);
        out.write_bool(state.vtimer_masked);
        out.bytes.extend_from_slice(&[0; 7]);
    }
    out.into_inner()
}

fn decode_vcpus(data: &[u8]) -> io::Result<Vec<VcpuRegisterBundle>> {
    let mut cursor = Cursor::new(data);
    let count = cursor.u32()? as usize;
    let mut vcpus = Vec::with_capacity(count);

    for _ in 0..count {
        let mut x = [0u64; 31];
        for value in &mut x {
            *value = cursor.u64()?;
        }
        let pc = cursor.u64()?;
        let fpcr = cursor.u64()?;
        let fpsr = cursor.u64()?;
        let cpsr = cursor.u64()?;

        let sys_count = cursor.u32()? as usize;
        let mut sys_regs = Vec::with_capacity(sys_count);
        for _ in 0..sys_count {
            let reg = cursor.u16()?;
            if cursor.u16()? != 0 || cursor.u32()? != 0 {
                return Err(invalid("invalid VCPU system-register entry"));
            }
            sys_regs.push((reg, cursor.u64()?));
        }

        let mut simd = [[0u8; 16]; 32];
        for value in &mut simd {
            value.copy_from_slice(cursor.take(16)?);
        }

        let gic_count = cursor.u32()? as usize;
        let mut gic_icc_regs = Vec::with_capacity(gic_count);
        for _ in 0..gic_count {
            let reg = cursor.u16()?;
            if cursor.u16()? != 0 || cursor.u32()? != 0 {
                return Err(invalid("invalid VCPU GIC-register entry"));
            }
            gic_icc_regs.push((reg, cursor.u64()?));
        }

        let vtimer_offset = cursor.u64()?;
        let vtimer_masked = match cursor.u8()? {
            0 => false,
            1 => true,
            _ => return Err(invalid("invalid vtimer mask value")),
        };
        if cursor.take(7)?.iter().any(|byte| *byte != 0) {
            return Err(invalid("invalid VCPU reserved bytes"));
        }

        vcpus.push(VcpuRegisterBundle {
            x,
            pc,
            fpcr,
            fpsr,
            cpsr,
            sys_regs,
            simd,
            gic_icc_regs,
            vtimer_offset,
            vtimer_masked,
        });
    }

    if !cursor.is_finished() {
        return Err(invalid("trailing bytes in VCPU section"));
    }
    Ok(vcpus)
}

fn write_section(file: &mut File, tag: [u8; 4], payload: &[u8]) -> io::Result<()> {
    file.write_all(&tag)?;
    file.write_all(&0u32.to_le_bytes())?;
    file.write_all(&(payload.len() as u64).to_le_bytes())?;
    file.write_all(payload)
}

fn read_section(file: &mut File) -> io::Result<([u8; 4], Vec<u8>)> {
    let mut header = [0u8; 16];
    file.read_exact(&mut header)?;
    let tag = header[..4].try_into().unwrap();
    let flags = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let len = u64::from_le_bytes(header[8..16].try_into().unwrap());
    if flags != 0 || len > MAX_SECTION_BYTES {
        return Err(invalid("invalid checkpoint section header"));
    }
    let len = usize::try_from(len).map_err(|_| invalid("section length does not fit usize"))?;
    let mut payload = vec![0u8; len];
    file.read_exact(&mut payload)?;
    Ok((tag, payload))
}

#[derive(Debug)]
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, len: usize) -> io::Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| invalid("checkpoint cursor overflow"))?;
        if end > self.bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "truncated checkpoint",
            ));
        }
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn blob(&mut self) -> io::Result<Vec<u8>> {
        let len = usize::try_from(self.u64()?)
            .map_err(|_| invalid("blob length does not fit usize"))?;
        Ok(self.take(len)?.to_vec())
    }

    fn is_finished(&self) -> bool {
        self.pos == self.bytes.len()
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn get_reg(vcpu: HvVcpu, reg: u32, value: &mut u64) -> io::Result<()> {
    hv(
        unsafe { hv_vcpu_get_reg(vcpu, reg, value) },
        "hv_vcpu_get_reg",
    )
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn set_reg(vcpu: HvVcpu, reg: u32, value: u64) -> io::Result<()> {
    hv(
        unsafe { hv_vcpu_set_reg(vcpu, reg, value) },
        "hv_vcpu_set_reg",
    )
}

fn hv(status: i32, operation: &str) -> io::Result<()> {
    if status == HV_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("{operation} failed with HVF status {status:#x}"),
        ))
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "HVF checkpointing requires macOS on arm64",
    )
}
