use std::{
    alloc::{alloc_zeroed, dealloc, Layout},
    sync::{Arc, Mutex},
};

use crate::virtio_gpu_3d::{
    CapsetInfo, CompletedFence, CreateBlobArgs, CtrlHdr3d, GpuShmMapPort, MappedBlob,
    ScanoutMappedBlob, VirtioGpu3d, VirtioGpu3dBackend, VIRTIO_GPU_BLOB_MEM_HOST3D,
    VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, VIRTIO_GPU_CMD_CTX_CREATE, VIRTIO_GPU_CMD_GET_CAPSET,
    VIRTIO_GPU_CMD_GET_CAPSET_INFO, VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB,
    VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB, VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB, VIRTIO_GPU_CMD_SUBMIT_3D,
    VIRTIO_GPU_MAP_CACHE_MASK, VIRTIO_GPU_RESP_OK_CAPSET, VIRTIO_GPU_RESP_OK_CAPSET_INFO,
    VIRTIO_GPU_RESP_OK_MAP_INFO, VIRTIO_GPU_RESP_OK_NODATA,
};

const VIRTIO_GPU_CAPSET_VENUS: u32 = 4;
const VIRTIO_GPU_CAPSET_VIRGL: u32 = 1;
const HOST_VISIBLE_SHM_OFFSET: u64 = 0x4000;
const HOST_VISIBLE_SHM_WINDOW: u64 = 0x1_0000;
const HOST_BLOB_RESOURCE_ID: u32 = 7;
const HOST_BLOB_SIZE: u64 = 4096;
const HVF_PAGE_SIZE: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioGpu3dHostPreflightProtocol {
    Venus,
    Virgl,
}

impl VirtioGpu3dHostPreflightProtocol {
    pub fn label(self) -> &'static str {
        match self {
            Self::Venus => "venus",
            Self::Virgl => "virgl",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Venus => "VENUS",
            Self::Virgl => "VIRGL",
        }
    }

    fn capset_id(self) -> u32 {
        match self {
            Self::Venus => VIRTIO_GPU_CAPSET_VENUS,
            Self::Virgl => VIRTIO_GPU_CAPSET_VIRGL,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtioGpu3dHostPreflight {
    pub protocol: VirtioGpu3dHostPreflightProtocol,
    pub capset_info_ok: bool,
    pub venus_capset_id: bool,
    pub virgl_capset_id: bool,
    pub expected_capset_id_ok: bool,
    pub capset_ok: bool,
    pub ctx_create_ok: bool,
    pub blob_create_ok: bool,
    pub blob_map_ok: bool,
    pub shm_map_called: bool,
    pub ctx_attach_ok: bool,
    pub submit_3d_ok: bool,
    pub fence_create_ok: bool,
    pub fence_completed: bool,
    pub blob_unmap_ok: bool,
    pub shm_unmap_called: bool,
    pub backend_unmap_called: bool,
    pub submit_bytes: usize,
    pub map_info: Option<u32>,
    pub blockers: Vec<String>,
}

impl VirtioGpu3dHostPreflight {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF virtio-gpu 3D host preflight\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: not entered\n");
        output.push_str("Renderer: synthetic host-visible blob backend\n");
        output.push_str(&format!("Requested protocol: {}\n", self.protocol.label()));
        output.push_str(&format!("GET_CAPSET_INFO OK: {}\n", self.capset_info_ok));
        output.push_str(&format!("VENUS capset id 4: {}\n", self.venus_capset_id));
        output.push_str(&format!("VIRGL capset id 1: {}\n", self.virgl_capset_id));
        output.push_str(&format!(
            "{} expected capset id {}: {}\n",
            self.protocol.display_name(),
            self.protocol.capset_id(),
            self.expected_capset_id_ok
        ));
        output.push_str(&format!("GET_CAPSET OK: {}\n", self.capset_ok));
        output.push_str(&format!("CTX_CREATE OK: {}\n", self.ctx_create_ok));
        output.push_str(&format!(
            "RESOURCE_CREATE_BLOB OK: {}\n",
            self.blob_create_ok
        ));
        output.push_str(&format!("RESOURCE_MAP_BLOB OK: {}\n", self.blob_map_ok));
        output.push_str(&format!("SHM map called: {}\n", self.shm_map_called));
        output.push_str(&format!("CTX_ATTACH_RESOURCE OK: {}\n", self.ctx_attach_ok));
        output.push_str(&format!("SUBMIT_3D OK: {}\n", self.submit_3d_ok));
        output.push_str(&format!("SUBMIT_3D bytes: {}\n", self.submit_bytes));
        output.push_str(&format!("Fence create OK: {}\n", self.fence_create_ok));
        output.push_str(&format!("Fence completed: {}\n", self.fence_completed));
        output.push_str(&format!("RESOURCE_UNMAP_BLOB OK: {}\n", self.blob_unmap_ok));
        output.push_str(&format!("SHM unmap called: {}\n", self.shm_unmap_called));
        output.push_str(&format!(
            "Backend unmap called: {}\n",
            self.backend_unmap_called
        ));
        output.push_str(&format!(
            "Map info: {}\n",
            self.map_info
                .map(|value| format!("{value:#x}"))
                .unwrap_or_else(|| "missing".to_string())
        ));
        if self.blockers.is_empty() {
            output.push_str("Blockers: none\n");
        } else {
            output.push_str("Blockers:\n");
            for blocker in &self.blockers {
                output.push_str(&format!("- {blocker}\n"));
            }
        }
        output
    }
}

pub fn probe_virtio_gpu_3d_host_preflight() -> VirtioGpu3dHostPreflight {
    probe_virtio_gpu_3d_host_preflight_for(VirtioGpu3dHostPreflightProtocol::Venus)
}

pub fn probe_virtio_gpu_3d_host_preflight_for(
    protocol: VirtioGpu3dHostPreflightProtocol,
) -> VirtioGpu3dHostPreflight {
    let allocation = match AlignedAllocation::new(HVF_PAGE_SIZE, HVF_PAGE_SIZE) {
        Some(allocation) => Arc::new(allocation),
        None => {
            return VirtioGpu3dHostPreflight {
                protocol,
                capset_info_ok: false,
                venus_capset_id: false,
                virgl_capset_id: false,
                expected_capset_id_ok: false,
                capset_ok: false,
                ctx_create_ok: false,
                blob_create_ok: false,
                blob_map_ok: false,
                shm_map_called: false,
                ctx_attach_ok: false,
                submit_3d_ok: false,
                fence_create_ok: false,
                fence_completed: false,
                blob_unmap_ok: false,
                shm_unmap_called: false,
                backend_unmap_called: false,
                submit_bytes: 0,
                map_info: None,
                blockers: vec!["failed to allocate aligned host-visible blob".to_string()],
            };
        }
    };

    let backend = Arc::new(Mutex::new(PreflightBackend::new(
        allocation,
        protocol.capset_id(),
    )));
    let port = Arc::new(Mutex::new(PreflightMapPort::default()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    gpu.set_shm_map_port(Box::new(port.clone()), HOST_VISIBLE_SHM_WINDOW);

    let capset_info_response = handle(&mut gpu, &get_capset_info_req(0));
    let capset_info_ok =
        response_type(&capset_info_response) == Some(VIRTIO_GPU_RESP_OK_CAPSET_INFO);
    let venus_capset_id = read_le_u32(&capset_info_response, 24) == Some(VIRTIO_GPU_CAPSET_VENUS);
    let virgl_capset_id = read_le_u32(&capset_info_response, 24) == Some(VIRTIO_GPU_CAPSET_VIRGL);
    let expected_capset_id_ok =
        read_le_u32(&capset_info_response, 24) == Some(protocol.capset_id());

    let capset_response = handle(&mut gpu, &get_capset_req(protocol.capset_id(), 1));
    let capset_ok = response_type(&capset_response) == Some(VIRTIO_GPU_RESP_OK_CAPSET);

    let ctx_create_response = handle(
        &mut gpu,
        &ctx_create_req(1, protocol.capset_id(), b"p3-host-preflight"),
    );
    let ctx_create_ok = response_type(&ctx_create_response) == Some(VIRTIO_GPU_RESP_OK_NODATA);

    let blob_create_response = handle(
        &mut gpu,
        &create_blob_req(
            HOST_BLOB_RESOURCE_ID,
            VIRTIO_GPU_BLOB_MEM_HOST3D,
            0x7000,
            HOST_BLOB_SIZE,
            1,
        ),
    );
    let blob_create_ok = response_type(&blob_create_response) == Some(VIRTIO_GPU_RESP_OK_NODATA);

    let blob_map_response = handle(
        &mut gpu,
        &map_blob_req(HOST_BLOB_RESOURCE_ID, HOST_VISIBLE_SHM_OFFSET),
    );
    let blob_map_ok = response_type(&blob_map_response) == Some(VIRTIO_GPU_RESP_OK_MAP_INFO);
    let map_info = read_le_u32(&blob_map_response, 24);

    let ctx_attach_response = handle(
        &mut gpu,
        &ctx_resource_req(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, 1, HOST_BLOB_RESOURCE_ID),
    );
    let ctx_attach_ok = response_type(&ctx_attach_response) == Some(VIRTIO_GPU_RESP_OK_NODATA);

    let submit_payload = [0xaa, 0xbb, 0xcc, 0xdd, 0x10, 0x20, 0x30, 0x40];
    let submit_response = handle(&mut gpu, &submit_3d_req(1, &submit_payload));
    let submit_3d_ok = response_type(&submit_response) == Some(VIRTIO_GPU_RESP_OK_NODATA);

    let fence = CompletedFence {
        ctx_id: 1,
        ring_idx: 0,
        fence_id: 9,
    };
    let fence_create_ok = gpu.create_fence(fence);
    let completed = gpu.drain_completed_fences();
    let fence_completed = completed.contains(&fence);

    let blob_unmap_response = handle(&mut gpu, &unmap_blob_req(HOST_BLOB_RESOURCE_ID));
    let blob_unmap_ok = response_type(&blob_unmap_response) == Some(VIRTIO_GPU_RESP_OK_NODATA);

    let backend_state = backend.lock().unwrap();
    let port_state = port.lock().unwrap();
    let submit_bytes = backend_state
        .submits
        .iter()
        .map(|(_, bytes)| bytes.len())
        .sum();
    let shm_map_called = port_state.maps.len() == 1
        && port_state.maps[0].1 == HVF_PAGE_SIZE
        && port_state.maps[0].2 == HOST_VISIBLE_SHM_OFFSET;
    let shm_unmap_called = port_state.unmaps.len() == 1
        && port_state.unmaps[0].0 == HOST_VISIBLE_SHM_OFFSET
        && port_state.unmaps[0].1 == HVF_PAGE_SIZE;
    let backend_unmap_called = backend_state.unmapped == vec![HOST_BLOB_RESOURCE_ID];
    let mut blockers = Vec::new();
    push_blocker(&mut blockers, capset_info_ok, "GET_CAPSET_INFO failed");
    push_blocker(
        &mut blockers,
        expected_capset_id_ok,
        &format!(
            "GET_CAPSET_INFO did not report {} capset id {}",
            protocol.display_name(),
            protocol.capset_id()
        ),
    );
    push_blocker(&mut blockers, capset_ok, "GET_CAPSET failed");
    push_blocker(&mut blockers, ctx_create_ok, "CTX_CREATE failed");
    push_blocker(&mut blockers, blob_create_ok, "RESOURCE_CREATE_BLOB failed");
    push_blocker(&mut blockers, blob_map_ok, "RESOURCE_MAP_BLOB failed");
    push_blocker(
        &mut blockers,
        shm_map_called,
        "host-visible SHM map was not called",
    );
    push_blocker(&mut blockers, ctx_attach_ok, "CTX_ATTACH_RESOURCE failed");
    push_blocker(&mut blockers, submit_3d_ok, "SUBMIT_3D failed");
    push_blocker(
        &mut blockers,
        submit_bytes == submit_payload.len(),
        "SUBMIT_3D payload was not delivered to backend",
    );
    push_blocker(
        &mut blockers,
        fence_create_ok,
        "renderer fence create failed",
    );
    push_blocker(
        &mut blockers,
        fence_completed,
        "renderer fence did not complete",
    );
    push_blocker(&mut blockers, blob_unmap_ok, "RESOURCE_UNMAP_BLOB failed");
    push_blocker(
        &mut blockers,
        shm_unmap_called,
        "host-visible SHM unmap was not called",
    );
    push_blocker(
        &mut blockers,
        backend_unmap_called,
        "backend blob unmap was not called",
    );
    push_blocker(
        &mut blockers,
        map_info.is_some_and(|info| info & VIRTIO_GPU_MAP_CACHE_MASK == 0x3),
        "RESOURCE_MAP_BLOB did not return expected map info",
    );

    VirtioGpu3dHostPreflight {
        protocol,
        capset_info_ok,
        venus_capset_id,
        virgl_capset_id,
        expected_capset_id_ok,
        capset_ok,
        ctx_create_ok,
        blob_create_ok,
        blob_map_ok,
        shm_map_called,
        ctx_attach_ok,
        submit_3d_ok,
        fence_create_ok,
        fence_completed,
        blob_unmap_ok,
        shm_unmap_called,
        backend_unmap_called,
        submit_bytes,
        map_info,
        blockers,
    }
}

fn handle(gpu: &mut VirtioGpu3d, request: &[u8]) -> Vec<u8> {
    let hdr = CtrlHdr3d::parse(request).expect("synthetic request header");
    gpu.handle(request, hdr)
        .expect("synthetic virtio-gpu 3D request")
}

fn push_blocker(blockers: &mut Vec<String>, ok: bool, blocker: &str) {
    if !ok {
        blockers.push(blocker.to_string());
    }
}

fn response_type(response: &[u8]) -> Option<u32> {
    read_le_u32(response, 0)
}

fn read_le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn ctrl_req(typ: u32, ctx_id: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&typ.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&ctx_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

fn get_capset_info_req(index: u32) -> Vec<u8> {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET_INFO, 0);
    req.extend_from_slice(&index.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req
}

fn get_capset_req(capset_id: u32, version: u32) -> Vec<u8> {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET, 0);
    req.extend_from_slice(&capset_id.to_le_bytes());
    req.extend_from_slice(&version.to_le_bytes());
    req
}

fn ctx_create_req(ctx_id: u32, context_init: u32, name: &[u8]) -> Vec<u8> {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_CTX_CREATE, ctx_id);
    let nlen = name.len().min(64);
    req.extend_from_slice(&(nlen as u32).to_le_bytes());
    req.extend_from_slice(&context_init.to_le_bytes());
    let mut debug_name = [0u8; 64];
    debug_name[..nlen].copy_from_slice(&name[..nlen]);
    req.extend_from_slice(&debug_name);
    req
}

fn create_blob_req(
    resource_id: u32,
    blob_mem: u32,
    blob_id: u64,
    size: u64,
    ctx_id: u32,
) -> Vec<u8> {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB, ctx_id);
    req.extend_from_slice(&resource_id.to_le_bytes());
    req.extend_from_slice(&blob_mem.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(&blob_id.to_le_bytes());
    req.extend_from_slice(&size.to_le_bytes());
    req
}

fn map_blob_req(resource_id: u32, offset: u64) -> Vec<u8> {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB, 0);
    req.extend_from_slice(&resource_id.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(&offset.to_le_bytes());
    req
}

fn unmap_blob_req(resource_id: u32) -> Vec<u8> {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB, 0);
    req.extend_from_slice(&resource_id.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req
}

fn ctx_resource_req(typ: u32, ctx_id: u32, resource_id: u32) -> Vec<u8> {
    let mut req = ctrl_req(typ, ctx_id);
    req.extend_from_slice(&resource_id.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req
}

fn submit_3d_req(ctx_id: u32, payload: &[u8]) -> Vec<u8> {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_SUBMIT_3D, ctx_id);
    req.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(payload);
    req
}

struct AlignedAllocation {
    ptr: usize,
    layout: Layout,
}

impl AlignedAllocation {
    fn new(size: usize, align: usize) -> Option<Self> {
        let layout = Layout::from_size_align(size, align).ok()?;
        // SAFETY: `layout` is non-zero and valid. The allocation is released in Drop.
        let ptr = unsafe { alloc_zeroed(layout) };
        (!ptr.is_null()).then_some(Self {
            ptr: ptr as usize,
            layout,
        })
    }

    fn ptr(&self) -> *mut u8 {
        self.ptr as *mut u8
    }
}

impl Drop for AlignedAllocation {
    fn drop(&mut self) {
        // SAFETY: `ptr` was allocated with this exact layout in `new`.
        unsafe { dealloc(self.ptr as *mut u8, self.layout) };
    }
}

struct PreflightBackend {
    allocation: Arc<AlignedAllocation>,
    capset_id: u32,
    submits: Vec<(u32, Vec<u8>)>,
    unmapped: Vec<u32>,
    completed: Vec<CompletedFence>,
}

impl PreflightBackend {
    fn new(allocation: Arc<AlignedAllocation>, capset_id: u32) -> Self {
        Self {
            allocation,
            capset_id,
            submits: Vec::new(),
            unmapped: Vec::new(),
            completed: Vec::new(),
        }
    }
}

impl VirtioGpu3dBackend for Arc<Mutex<PreflightBackend>> {
    fn capset_info(&mut self, capset_index: u32) -> Option<CapsetInfo> {
        let capset_id = self.lock().unwrap().capset_id;
        (capset_index == 0).then_some(CapsetInfo {
            capset_id,
            max_version: 1,
            max_size: 160,
        })
    }

    fn capset(&mut self, capset_id: u32, version: u32) -> Option<Vec<u8>> {
        if capset_id != self.lock().unwrap().capset_id || version != 1 {
            return None;
        }
        let mut capset = vec![0u8; 160];
        capset[0..4].copy_from_slice(&1u32.to_le_bytes());
        Some(capset)
    }

    fn ctx_create(&mut self, _ctx_id: u32, context_init: u32, _name: &[u8]) -> bool {
        context_init & 0xff == self.lock().unwrap().capset_id
    }

    fn ctx_destroy(&mut self, _ctx_id: u32) {}

    fn ctx_attach_resource(&mut self, _ctx_id: u32, _resource_id: u32) {}

    fn ctx_detach_resource(&mut self, _ctx_id: u32, _resource_id: u32) {}

    fn submit_3d(&mut self, ctx_id: u32, cmdbuf: &[u8]) -> bool {
        if cmdbuf.is_empty() {
            return false;
        }
        self.lock().unwrap().submits.push((ctx_id, cmdbuf.to_vec()));
        true
    }

    fn create_blob(&mut self, args: CreateBlobArgs<'_>) -> bool {
        args.ctx_id == 1
            && args.resource_id == HOST_BLOB_RESOURCE_ID
            && args.blob_mem == VIRTIO_GPU_BLOB_MEM_HOST3D
            && args.size == HOST_BLOB_SIZE
            && args.iovecs.is_empty()
    }

    fn map_blob(&mut self, resource_id: u32) -> Option<MappedBlob> {
        (resource_id == HOST_BLOB_RESOURCE_ID).then(|| {
            let inner = self.lock().unwrap();
            MappedBlob {
                host_ptr: inner.allocation.ptr(),
                size: HVF_PAGE_SIZE,
                map_info: 0x3,
            }
        })
    }

    fn unmap_blob(&mut self, resource_id: u32) {
        self.lock().unwrap().unmapped.push(resource_id);
    }

    fn scanout_map(&mut self, _resource_id: u32) -> Option<ScanoutMappedBlob> {
        None
    }

    fn scanout_unmap(&mut self, _resource_id: u32) {}

    fn destroy_resource(&mut self, _resource_id: u32) {}

    fn create_fence(&mut self, ctx_id: u32, ring_idx: u8, fence_id: u64) -> bool {
        let fence = CompletedFence {
            ctx_id,
            ring_idx,
            fence_id,
        };
        self.lock().unwrap().completed.push(fence);
        true
    }

    fn poll_fences(&mut self) {}

    fn drain_completed_fences(&mut self) -> Vec<CompletedFence> {
        std::mem::take(&mut self.lock().unwrap().completed)
    }

    fn reset(&mut self) {
        self.lock().unwrap().completed.clear();
    }
}

#[derive(Default)]
struct PreflightMapPort {
    maps: Vec<(usize, usize, u64)>,
    unmaps: Vec<(u64, usize)>,
}

impl GpuShmMapPort for Arc<Mutex<PreflightMapPort>> {
    fn map(&mut self, host_ptr: *mut u8, size: usize, shm_offset: u64) -> Result<(), i32> {
        self.lock()
            .unwrap()
            .maps
            .push((host_ptr as usize, size, shm_offset));
        Ok(())
    }

    fn unmap(&mut self, shm_offset: u64, size: usize) -> Result<(), i32> {
        self.lock().unwrap().unmaps.push((shm_offset, size));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtio_gpu_3d_host_preflight_passes_synthetic_blob_map_submit_fence_path() {
        let probe = probe_virtio_gpu_3d_host_preflight();

        assert!(probe.blockers.is_empty(), "{:?}", probe.blockers);
        assert_eq!(probe.protocol, VirtioGpu3dHostPreflightProtocol::Venus);
        assert!(probe.capset_info_ok);
        assert!(probe.venus_capset_id);
        assert!(probe.expected_capset_id_ok);
        assert!(probe.capset_ok);
        assert!(probe.ctx_create_ok);
        assert!(probe.blob_create_ok);
        assert!(probe.blob_map_ok);
        assert!(probe.shm_map_called);
        assert!(probe.ctx_attach_ok);
        assert!(probe.submit_3d_ok);
        assert_eq!(probe.submit_bytes, 8);
        assert!(probe.fence_create_ok);
        assert!(probe.fence_completed);
        assert!(probe.blob_unmap_ok);
        assert!(probe.shm_unmap_called);
        assert!(probe.backend_unmap_called);
        assert_eq!(probe.map_info, Some(0x3));
    }

    #[test]
    fn virtio_gpu_3d_host_preflight_can_request_virgl_capset_contract() {
        let probe = probe_virtio_gpu_3d_host_preflight_for(VirtioGpu3dHostPreflightProtocol::Virgl);

        assert!(probe.blockers.is_empty(), "{:?}", probe.blockers);
        assert_eq!(probe.protocol, VirtioGpu3dHostPreflightProtocol::Virgl);
        assert!(probe.capset_info_ok);
        assert!(!probe.venus_capset_id);
        assert!(probe.virgl_capset_id);
        assert!(probe.expected_capset_id_ok);
        assert!(probe.capset_ok);
        assert!(probe.ctx_create_ok);
        assert!(probe.submit_3d_ok);
        assert_eq!(probe.submit_bytes, 8);
        assert!(probe.fence_completed);
    }

    #[test]
    fn virtio_gpu_3d_host_preflight_render_text_reports_contract() {
        let output = probe_virtio_gpu_3d_host_preflight().render_text();

        assert!(output.contains("HVF virtio-gpu 3D host preflight"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Guest execution: not entered"));
        assert!(output.contains("Renderer: synthetic host-visible blob backend"));
        assert!(output.contains("Requested protocol: venus"));
        assert!(output.contains("VENUS capset id 4: true"));
        assert!(output.contains("VENUS expected capset id 4: true"));
        assert!(output.contains("RESOURCE_MAP_BLOB OK: true"));
        assert!(output.contains("Fence completed: true"));
        assert!(output.contains("Blockers: none"));
    }
}
