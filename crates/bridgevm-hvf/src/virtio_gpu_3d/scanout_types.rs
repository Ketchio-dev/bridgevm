//! Plain scanout result types shared by renderer implementations.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanoutPresentResult {
    pub surface_id: Option<u32>,
    pub readback_ok: Option<bool>,
    pub blit_duration_ns: u64,
    pub readback_duration_ns: u64,
}
