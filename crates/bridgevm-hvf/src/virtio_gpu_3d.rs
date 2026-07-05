use std::collections::{BTreeMap, BTreeSet};

pub const VIRTIO_GPU_F_VIRGL: u32 = 1 << 0;
pub const VIRTIO_GPU_F_CONTEXT_INIT: u32 = 1 << 4;

pub const VIRTIO_GPU_FLAG_FENCE: u32 = 1;
pub const VIRTIO_GPU_FLAG_INFO_RING_IDX: u32 = 1 << 1;

pub const VIRTIO_GPU_CMD_GET_CAPSET_INFO: u32 = 0x0108;
pub const VIRTIO_GPU_CMD_GET_CAPSET: u32 = 0x0109;
pub const VIRTIO_GPU_CMD_CTX_CREATE: u32 = 0x0200;
pub const VIRTIO_GPU_CMD_CTX_DESTROY: u32 = 0x0201;
pub const VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE: u32 = 0x0202;
pub const VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE: u32 = 0x0203;
pub const VIRTIO_GPU_CMD_SUBMIT_3D: u32 = 0x0207;

pub const VIRTIO_GPU_RESP_OK_NODATA: u32 = 0x1100;
pub const VIRTIO_GPU_RESP_OK_CAPSET_INFO: u32 = 0x1102;
pub const VIRTIO_GPU_RESP_OK_CAPSET: u32 = 0x1103;
pub const VIRTIO_GPU_RESP_ERR_UNSPEC: u32 = 0x1200;
pub const VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER: u32 = 0x1203;

const CTRL_HDR_LEN: usize = 24;
const CTX_CREATE_LEN: usize = 24 + 4 + 4 + 64;
const SUBMIT_3D_LEN: usize = 24 + 4 + 4;
const MAX_SUBMIT_3D_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapsetInfo {
    pub capset_id: u32,
    pub max_version: u32,
    pub max_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletedFence {
    pub ctx_id: u32,
    pub ring_idx: u8,
    pub fence_id: u64,
}

pub trait VirtioGpu3dBackend: Send {
    fn capset_info(&mut self, capset_index: u32) -> Option<CapsetInfo>;
    fn capset(&mut self, capset_id: u32, version: u32) -> Option<Vec<u8>>;
    fn ctx_create(&mut self, ctx_id: u32, context_init: u32, name: &[u8]) -> bool;
    fn ctx_destroy(&mut self, ctx_id: u32);
    fn submit_3d(&mut self, ctx_id: u32, cmdbuf: &[u8]) -> bool;
    fn create_fence(&mut self, ctx_id: u32, ring_idx: u8, fence_id: u64) -> bool;
    fn drain_completed_fences(&mut self) -> Vec<CompletedFence>;
    fn reset(&mut self);
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VirtioGpu3dStats {
    pub ctx_active: usize,
    pub submits: u64,
    pub fences_pending: usize,
    pub fences_completed: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct CtrlHdr3d {
    pub typ: u32,
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
    pub ring_idx: u8,
    pub padding: u32,
}

impl CtrlHdr3d {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let padding = read_le_u32(bytes, 20)?;
        Some(Self {
            typ: read_le_u32(bytes, 0)?,
            flags: read_le_u32(bytes, 4)?,
            fence_id: read_le_u64(bytes, 8)?,
            ctx_id: read_le_u32(bytes, 16)?,
            ring_idx: if read_le_u32(bytes, 4)? & VIRTIO_GPU_FLAG_INFO_RING_IDX != 0 {
                (padding & 0xff) as u8
            } else {
                0
            },
            padding,
        })
    }

    pub fn fenced(self) -> bool {
        self.flags & VIRTIO_GPU_FLAG_FENCE != 0
    }
}

#[derive(Default)]
pub struct VirtioGpu3d {
    backend: Option<Box<dyn VirtioGpu3dBackend>>,
    live_contexts: BTreeSet<u32>,
    ctx_resources: BTreeMap<u32, BTreeSet<u32>>,
    submits: u64,
    fences_completed: u64,
}

impl std::fmt::Debug for VirtioGpu3d {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtioGpu3d")
            .field("has_backend", &self.backend.is_some())
            .field("live_contexts", &self.live_contexts)
            .field("ctx_resources", &self.ctx_resources)
            .field("submits", &self.submits)
            .field("fences_completed", &self.fences_completed)
            .finish()
    }
}

impl VirtioGpu3d {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_backend(backend: Box<dyn VirtioGpu3dBackend>) -> Self {
        Self {
            backend: Some(backend),
            ..Self::default()
        }
    }

    pub fn has_backend(&self) -> bool {
        self.backend.is_some()
    }

    pub fn stats(&self, fences_pending: usize) -> VirtioGpu3dStats {
        VirtioGpu3dStats {
            ctx_active: self.live_contexts.len(),
            submits: self.submits,
            fences_pending,
            fences_completed: self.fences_completed,
        }
    }

    pub fn reset(&mut self) {
        if let Some(backend) = self.backend.as_mut() {
            backend.reset();
        }
        self.live_contexts.clear();
        self.ctx_resources.clear();
        self.submits = 0;
    }

    pub fn drain_completed_fences(&mut self) -> Vec<CompletedFence> {
        let Some(backend) = self.backend.as_mut() else {
            return Vec::new();
        };
        let completed = backend.drain_completed_fences();
        self.fences_completed = self.fences_completed.saturating_add(completed.len() as u64);
        completed
    }

    pub fn create_fence(&mut self, fence: CompletedFence) -> bool {
        let Some(backend) = self.backend.as_mut() else {
            return false;
        };
        backend.create_fence(fence.ctx_id, fence.ring_idx, fence.fence_id)
    }

    pub fn handle(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Option<Vec<u8>> {
        match hdr.typ {
            VIRTIO_GPU_CMD_GET_CAPSET_INFO => Some(self.get_capset_info(request, hdr)),
            VIRTIO_GPU_CMD_GET_CAPSET => Some(self.get_capset(request, hdr)),
            VIRTIO_GPU_CMD_CTX_CREATE => Some(self.ctx_create(request, hdr)),
            VIRTIO_GPU_CMD_CTX_DESTROY => Some(self.ctx_destroy(hdr)),
            VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE => Some(self.ctx_attach_resource(request, hdr)),
            VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE => Some(self.ctx_detach_resource(request, hdr)),
            VIRTIO_GPU_CMD_SUBMIT_3D => Some(self.submit_3d(request, hdr)),
            _ => None,
        }
    }

    fn get_capset_info(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        let Some(backend) = self.backend.as_mut() else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let Some(index) = read_le_u32(request, 24) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let Some(info) = backend.capset_info(index) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let mut out = response_hdr(VIRTIO_GPU_RESP_OK_CAPSET_INFO, Some(hdr));
        out.extend_from_slice(&info.capset_id.to_le_bytes());
        out.extend_from_slice(&info.max_version.to_le_bytes());
        out.extend_from_slice(&info.max_size.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    fn get_capset(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        let Some(backend) = self.backend.as_mut() else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let Some(capset_id) = read_le_u32(request, 24) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let Some(version) = read_le_u32(request, 28) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let Some(bytes) = backend.capset(capset_id, version) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let mut out = response_hdr(VIRTIO_GPU_RESP_OK_CAPSET, Some(hdr));
        out.extend_from_slice(&bytes);
        out
    }

    fn ctx_create(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        let Some(backend) = self.backend.as_mut() else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        if request.len() < CTX_CREATE_LEN
            || hdr.ctx_id == 0
            || self.live_contexts.contains(&hdr.ctx_id)
        {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let nlen = read_le_u32(request, 24).unwrap_or(0).min(64) as usize;
        let context_init = read_le_u32(request, 28).unwrap_or(0);
        let name = &request[32..32 + nlen];
        if !backend.ctx_create(hdr.ctx_id, context_init, name) {
            return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
        }
        self.live_contexts.insert(hdr.ctx_id);
        self.ctx_resources.entry(hdr.ctx_id).or_default();
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, Some(hdr))
    }

    fn ctx_destroy(&mut self, hdr: CtrlHdr3d) -> Vec<u8> {
        if !self.live_contexts.remove(&hdr.ctx_id) {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        self.ctx_resources.remove(&hdr.ctx_id);
        if let Some(backend) = self.backend.as_mut() {
            backend.ctx_destroy(hdr.ctx_id);
        }
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, Some(hdr))
    }

    fn ctx_attach_resource(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        if !self.live_contexts.contains(&hdr.ctx_id) {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let Some(resource_id) = read_le_u32(request, 24) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        self.ctx_resources
            .entry(hdr.ctx_id)
            .or_default()
            .insert(resource_id);
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, Some(hdr))
    }

    fn ctx_detach_resource(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        if !self.live_contexts.contains(&hdr.ctx_id) {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        if let Some(resource_id) = read_le_u32(request, 24) {
            if let Some(resources) = self.ctx_resources.get_mut(&hdr.ctx_id) {
                resources.remove(&resource_id);
            }
        }
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, Some(hdr))
    }

    fn submit_3d(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        let Some(backend) = self.backend.as_mut() else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        if !self.live_contexts.contains(&hdr.ctx_id) || request.len() < SUBMIT_3D_LEN {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let size = read_le_u32(request, 24).unwrap_or(0) as usize;
        if size > MAX_SUBMIT_3D_BYTES || request.len().saturating_sub(SUBMIT_3D_LEN) < size {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let cmdbuf = &request[SUBMIT_3D_LEN..SUBMIT_3D_LEN + size];
        if !backend.submit_3d(hdr.ctx_id, cmdbuf) {
            return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
        }
        self.submits = self.submits.saturating_add(1);
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, Some(hdr))
    }
}

pub fn response_hdr(typ: u32, request: Option<CtrlHdr3d>) -> Vec<u8> {
    let mut out = Vec::with_capacity(CTRL_HDR_LEN);
    let (flags, fence_id, ctx_id, padding) = request.map_or((0, 0, 0, 0), |hdr| {
        (
            hdr.flags & (VIRTIO_GPU_FLAG_FENCE | VIRTIO_GPU_FLAG_INFO_RING_IDX),
            if hdr.fenced() { hdr.fence_id } else { 0 },
            hdr.ctx_id,
            hdr.padding,
        )
    });
    out.extend_from_slice(&typ.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&fence_id.to_le_bytes());
    out.extend_from_slice(&ctx_id.to_le_bytes());
    out.extend_from_slice(&padding.to_le_bytes());
    out
}

fn read_le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_le_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[cfg(test)]
#[derive(Debug, Default)]
pub struct MockBackend {
    pub capset_info: Option<CapsetInfo>,
    pub capset: Vec<u8>,
    pub created: Vec<(u32, u32, Vec<u8>)>,
    pub destroyed: Vec<u32>,
    pub submits: Vec<(u32, Vec<u8>)>,
    pub fences: Vec<CompletedFence>,
    pub completed: Vec<CompletedFence>,
}

#[cfg(test)]
impl MockBackend {
    pub fn new_venus() -> Self {
        let mut capset = vec![0u8; 160];
        capset[0..4].copy_from_slice(&1u32.to_le_bytes());
        Self {
            capset_info: Some(CapsetInfo {
                capset_id: 4,
                max_version: 1,
                max_size: 160,
            }),
            capset,
            ..Self::default()
        }
    }
}

#[cfg(test)]
impl VirtioGpu3dBackend for std::sync::Arc<std::sync::Mutex<MockBackend>> {
    fn capset_info(&mut self, capset_index: u32) -> Option<CapsetInfo> {
        (capset_index == 0)
            .then(|| self.lock().unwrap().capset_info)
            .flatten()
    }

    fn capset(&mut self, capset_id: u32, _version: u32) -> Option<Vec<u8>> {
        let inner = self.lock().unwrap();
        (inner.capset_info.map(|info| info.capset_id) == Some(capset_id))
            .then(|| inner.capset.clone())
    }

    fn ctx_create(&mut self, ctx_id: u32, context_init: u32, name: &[u8]) -> bool {
        self.lock()
            .unwrap()
            .created
            .push((ctx_id, context_init, name.to_vec()));
        true
    }

    fn ctx_destroy(&mut self, ctx_id: u32) {
        self.lock().unwrap().destroyed.push(ctx_id);
    }

    fn submit_3d(&mut self, ctx_id: u32, cmdbuf: &[u8]) -> bool {
        self.lock().unwrap().submits.push((ctx_id, cmdbuf.to_vec()));
        true
    }

    fn create_fence(&mut self, ctx_id: u32, ring_idx: u8, fence_id: u64) -> bool {
        self.lock().unwrap().fences.push(CompletedFence {
            ctx_id,
            ring_idx,
            fence_id,
        });
        true
    }

    fn drain_completed_fences(&mut self) -> Vec<CompletedFence> {
        std::mem::take(&mut self.lock().unwrap().completed)
    }

    fn reset(&mut self) {
        self.lock().unwrap().completed.clear();
    }
}
