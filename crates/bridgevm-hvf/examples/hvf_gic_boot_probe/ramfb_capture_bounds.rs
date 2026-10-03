//! Opt-in diagnostic evidence bounds, checked before allocation or file writes.
use bridgevm_hvf::{
    fwcfg::GuestMemoryMut,
    ramfb::{RamfbConfig, RamfbSnapshot, RamfbSnapshotError, RamfbSnapshotSummary},
};
use std::{
    fmt, fs,
    io::{self, Write},
    path::PathBuf,
};

pub(super) const RAW_LIMIT: u64 = 64 * 1024 * 1024;
pub(super) const PPM_LIMIT: u64 = 48 * 1024 * 1024 + 64;

pub(super) enum CaptureError {
    Snapshot(RamfbSnapshotError),
    ByteLimit { raw_bytes: u64, ppm_bytes: u64 },
    AddressRange { addr: u64, byte_len: u64 },
    Policy(io::Error),
}

impl fmt::Debug for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Snapshot(error) => fmt::Debug::fmt(error, f),
            Self::ByteLimit {
                raw_bytes,
                ppm_bytes,
            } => f
                .debug_struct("CaptureByteLimit")
                .field("raw_bytes", raw_bytes)
                .field("ppm_bytes", ppm_bytes)
                .finish(),
            Self::AddressRange { addr, byte_len } => f
                .debug_struct("CaptureAddressRange")
                .field("addr", addr)
                .field("byte_len", byte_len)
                .finish(),
            Self::Policy(error) => f.debug_tuple("CapturePolicyError").field(error).finish(),
        }
    }
}

impl From<RamfbSnapshotError> for CaptureError {
    fn from(error: RamfbSnapshotError) -> Self {
        Self::Snapshot(error)
    }
}

pub(super) struct Policy {
    pub(super) raw_limit: u64,
    pub(super) ppm_limit: u64,
    pub(super) directory: PathBuf,
}

impl Policy {
    pub(super) fn validate(&self, config: RamfbConfig) -> Result<(), CaptureError> {
        let result = geometry(config).and_then(|(raw_bytes, ppm_bytes)| {
            if raw_bytes > self.raw_limit || ppm_bytes > self.ppm_limit {
                Err(CaptureError::ByteLimit {
                    raw_bytes,
                    ppm_bytes,
                })
            } else {
                Ok(())
            }
        });
        let refused = matches!(
            result,
            Err(CaptureError::ByteLimit { .. })
                | Err(CaptureError::AddressRange { .. })
                | Err(CaptureError::Snapshot(RamfbSnapshotError::SizeOverflow))
        );
        self.record(refused).map_err(CaptureError::Policy)?;
        result
    }

    fn record(&self, refused: bool) -> io::Result<()> {
        static STATUS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = STATUS_LOCK
            .lock()
            .map_err(|_| io::Error::other("capture status lock poisoned"))?;
        fs::create_dir_all(&self.directory)?;
        let marker = self.directory.join("capture-bound-refused.json");
        if refused && !marker.exists() {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker)?;
            file.write_all(
                b"{\"schema\":\"bridgevm.ramfb-capture-refusal.v1\",\"refused\":true}\n",
            )?;
            file.sync_all()?;
        }
        let data = format!("{{\"schema\":\"bridgevm.ramfb-capture-bound.v1\",\"raw_limit_bytes\":{},\"ppm_limit_bytes\":{},\"complete\":true,\"refused\":{}}}\n",
                           self.raw_limit, self.ppm_limit, marker.exists());
        let temporary = self.directory.join("capture-bound-status.pending");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(data.as_bytes())?;
        file.sync_all()?;
        fs::rename(temporary, self.directory.join("capture-bound-status.json"))?;
        fs::File::open(&self.directory)?.sync_all()
    }
}

fn geometry(config: RamfbConfig) -> Result<(u64, u64), CaptureError> {
    if !config.is_active() {
        return Err(RamfbSnapshotError::Inactive.into());
    }
    if !config.is_xrgb8888() {
        return Err(RamfbSnapshotError::UnsupportedFormat {
            fourcc: config.fourcc,
        }
        .into());
    }
    let min_stride = u64::from(config.width)
        .checked_mul(4)
        .ok_or(RamfbSnapshotError::SizeOverflow)?;
    if u64::from(config.stride) < min_stride {
        return Err(RamfbSnapshotError::StrideTooSmall {
            stride: config.stride,
            min_stride,
        }
        .into());
    }
    let raw = u64::from(config.stride)
        .checked_mul(u64::from(config.height))
        .ok_or(RamfbSnapshotError::SizeOverflow)?;
    config
        .addr
        .checked_add(raw)
        .ok_or(CaptureError::AddressRange {
            addr: config.addr,
            byte_len: raw,
        })?;
    let pixels = u64::from(config.width)
        .checked_mul(u64::from(config.height))
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or(RamfbSnapshotError::SizeOverflow)?;
    let header = format!("P6\n{} {}\n255\n", config.width, config.height);
    let ppm = pixels
        .checked_add(header.len() as u64)
        .ok_or(RamfbSnapshotError::SizeOverflow)?;
    Ok((raw, ppm))
}

fn configured() -> Result<Option<Policy>, CaptureError> {
    let Some(value) = std::env::var_os("BRIDGEVM_DIAGNOSTIC_OUTPUT_BOUNDS") else {
        return Ok(None);
    };
    if value != "1" {
        return Err(CaptureError::Policy(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid diagnostic capture policy",
        )));
    }
    let directory = std::env::var_os("BRIDGEVM_RAMFB_DUMP_DIR").ok_or_else(|| {
        CaptureError::Policy(io::Error::new(
            io::ErrorKind::InvalidInput,
            "capture status directory required",
        ))
    })?;
    Ok(Some(Policy {
        raw_limit: RAW_LIMIT,
        ppm_limit: PPM_LIMIT,
        directory: directory.into(),
    }))
}

pub(super) fn read_with_policy(
    mem: &dyn GuestMemoryMut,
    config: RamfbConfig,
    policy: Option<&Policy>,
) -> Result<RamfbSnapshot, CaptureError> {
    if let Some(policy) = policy {
        policy.validate(config)?;
    }
    RamfbSnapshot::read_from(mem, config).map_err(Into::into)
}

pub(super) fn read(
    mem: &dyn GuestMemoryMut,
    config: RamfbConfig,
) -> Result<RamfbSnapshot, CaptureError> {
    read_with_policy(mem, config, configured()?.as_ref())
}

pub(super) fn summary(
    config: RamfbConfig,
    bytes: &[u8],
) -> Result<RamfbSnapshotSummary, CaptureError> {
    if let Some(policy) = configured()? {
        policy.validate(config)?;
    }
    RamfbSnapshot::summarize_xrgb8888_bytes(config, bytes).map_err(Into::into)
}

pub(super) fn ppm(config: RamfbConfig, bytes: &[u8]) -> Result<Vec<u8>, CaptureError> {
    if let Some(policy) = configured()? {
        policy.validate(config)?;
    }
    RamfbSnapshot::ppm_bytes_from_xrgb8888(config, bytes).map_err(Into::into)
}

#[cfg(test)]
#[path = "ramfb_capture_bounds_tests.rs"]
mod tests;
