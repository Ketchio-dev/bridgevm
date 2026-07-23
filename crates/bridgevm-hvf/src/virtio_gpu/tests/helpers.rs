//! Split test module.

use super::super::*;
use crate::virtio_gpu_3d::MockBackend;
use crate::{
    fwcfg::GuestMemoryMut,
    pcie::VIRTIO_GPU_MSIX_TABLE_OFFSET,
    virtio_gpu_3d::{
        self, VIRTIO_GPU_CMD_CTX_CREATE, VIRTIO_GPU_CMD_RESOURCE_CREATE_3D,
        VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB, VIRTIO_GPU_CMD_SUBMIT_3D, VIRTIO_GPU_FLAG_FENCE,
    },
};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub(super) struct TestMem {
    pub(super) base: u64,
    pub(super) bytes: Vec<u8>,
}

impl TestMem {
    pub(super) fn new(base: u64, len: usize) -> Self {
        Self {
            base,
            bytes: vec![0; len],
        }
    }

    pub(super) fn write(&mut self, gpa: u64, data: &[u8]) {
        assert!(self.write_bytes(gpa, data));
    }

    pub(super) fn read(&self, gpa: u64, len: usize) -> Vec<u8> {
        self.read_bytes(gpa, len).unwrap()
    }
}

impl GuestMemoryMut for TestMem {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(off) = gpa.checked_sub(self.base).map(|v| v as usize) else {
            return false;
        };
        let Some(end) = off.checked_add(data.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[off..end].copy_from_slice(data);
        true
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let off = gpa.checked_sub(self.base)? as usize;
        let end = off.checked_add(len)?;
        (end <= self.bytes.len()).then(|| self.bytes[off..end].to_vec())
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        let Some(off) = gpa.checked_sub(self.base).map(|v| v as usize) else {
            return false;
        };
        let Some(end) = off.checked_add(dst.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        dst.copy_from_slice(&self.bytes[off..end]);
        true
    }

    fn host_ptr(&self, gpa: u64, len: usize) -> Option<*mut u8> {
        let off = gpa.checked_sub(self.base)? as usize;
        let end = off.checked_add(len)?;
        (end <= self.bytes.len()).then(|| self.bytes.as_ptr().wrapping_add(off) as *mut u8)
    }
}

pub(super) fn pci_write(
    dev: &mut VirtioPciGpu,
    offset: u64,
    size: u8,
    value: u64,
    mem: &mut TestMem,
) {
    assert_eq!(
        dev.access(offset, VirtioPciGpuOp::Write { size, value }, mem),
        VirtioGpuResult::WriteAck
    );
}

pub(super) fn pci_read(dev: &mut VirtioPciGpu, offset: u64, size: u8, mem: &mut TestMem) -> u64 {
    match dev.access(offset, VirtioPciGpuOp::Read { size }, mem) {
        VirtioGpuResult::ReadValue(value) => value,
        VirtioGpuResult::WriteAck => panic!("read returned write ack"),
    }
}

pub(super) fn setup_queue(
    dev: &mut VirtioPciGpu,
    mem: &mut TestMem,
    queue: u16,
    desc: u64,
    avail: u64,
    used: u64,
    vector: u16,
) {
    pci_write(dev, COMMON_QUEUE_SELECT, 2, u64::from(queue), mem);
    pci_write(dev, COMMON_QUEUE_SIZE, 2, 16, mem);
    pci_write(dev, COMMON_QUEUE_DESC, 8, desc, mem);
    pci_write(dev, COMMON_QUEUE_DRIVER, 8, avail, mem);
    pci_write(dev, COMMON_QUEUE_DEVICE, 8, used, mem);
    pci_write(dev, COMMON_QUEUE_MSIX_VECTOR, 2, u64::from(vector), mem);
    pci_write(dev, COMMON_QUEUE_ENABLE, 2, 1, mem);
}

pub(super) fn program_msix_vector(dev: &mut VirtioPciGpu, vector: u16, address: u64, data: u32) {
    let off = u64::from(VIRTIO_GPU_MSIX_TABLE_OFFSET) + u64::from(vector) * 16;
    assert_eq!(
        dev.msix_bar_access(
            off,
            VirtioPciGpuOp::Write {
                size: 8,
                value: address,
            },
        ),
        VirtioGpuResult::WriteAck
    );
    assert_eq!(
        dev.msix_bar_access(
            off + 8,
            VirtioPciGpuOp::Write {
                size: 4,
                value: u64::from(data),
            },
        ),
        VirtioGpuResult::WriteAck
    );
    assert_eq!(
        dev.msix_bar_access(off + 12, VirtioPciGpuOp::Write { size: 4, value: 0 },),
        VirtioGpuResult::WriteAck
    );
}

pub(super) fn write_desc(
    mem: &mut TestMem,
    table: u64,
    index: u16,
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
) {
    let gpa = table + u64::from(index) * DESC_SIZE;
    mem.write(gpa, &addr.to_le_bytes());
    mem.write(gpa + 8, &len.to_le_bytes());
    mem.write(gpa + 12, &flags.to_le_bytes());
    mem.write(gpa + 14, &next.to_le_bytes());
}

pub(super) fn ctrl_req(typ: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&typ.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

pub(super) fn ctrl_req_ctx(typ: u32, ctx_id: u32) -> Vec<u8> {
    let mut out = ctrl_req(typ);
    out[16..20].copy_from_slice(&ctx_id.to_le_bytes());
    out
}

pub(super) fn create_blob_req(
    resource_id: u32,
    blob_mem: u32,
    size: u64,
    entries: &[(u64, u32)],
) -> Vec<u8> {
    let mut out = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB, 1);
    out.extend_from_slice(&resource_id.to_le_bytes());
    out.extend_from_slice(&blob_mem.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    for (addr, len) in entries {
        out.extend_from_slice(&addr.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    out
}

pub(super) fn set_scanout_blob_req(
    resource_id: u32,
    width: u32,
    height: u32,
    format: u32,
    stride: u32,
    offset: u32,
) -> Vec<u8> {
    let mut out = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT_BLOB);
    push_rect(
        &mut out,
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        },
    );
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&resource_id.to_le_bytes());
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&format.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for index in 0..4 {
        out.extend_from_slice(&(if index == 0 { stride } else { 0 }).to_le_bytes());
    }
    for index in 0..4 {
        out.extend_from_slice(&(if index == 0 { offset } else { 0 }).to_le_bytes());
    }
    out
}

pub(super) fn flush_req(resource_id: u32, rect: Rect) -> Vec<u8> {
    let mut out = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_FLUSH);
    push_rect(&mut out, rect);
    out.extend_from_slice(&resource_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

pub(super) fn ctrl_req_fenced(typ: u32, ctx_id: u32, ring_idx: u8, fence_id: u64) -> Vec<u8> {
    let mut out = ctrl_req_ctx(typ, ctx_id);
    out[4..8].copy_from_slice(
        &(VIRTIO_GPU_FLAG_FENCE | virtio_gpu_3d::VIRTIO_GPU_FLAG_INFO_RING_IDX).to_le_bytes(),
    );
    out[8..16].copy_from_slice(&fence_id.to_le_bytes());
    out[20] = ring_idx;
    out
}

pub(super) fn dev_with_mock() -> (VirtioPciGpu, Arc<Mutex<MockBackend>>) {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    (
        VirtioPciGpu::with_3d_backend(1280, 800, Box::new(backend.clone())),
        backend,
    )
}

pub(super) fn trace_test_path(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "bridgevm-virtio-gpu-{label}-{}-{nanos}.jsonl",
        std::process::id()
    ))
}

pub(super) fn submit_control(
    dev: &mut VirtioPciGpu,
    mem: &mut TestMem,
    request: &[u8],
    response_len: u32,
) -> Vec<u8> {
    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let req = 0x4000_4000;
    let resp = 0x4000_5000;
    setup_queue(dev, mem, 0, desc, avail, used, 0);
    let next_avail = dev.stats().queues[0].last_avail_idx.wrapping_add(1);
    let ring_slot = dev.stats().queues[0].last_avail_idx % 16;
    mem.write(req, request);
    write_desc(mem, desc, 0, req, request.len() as u32, DESC_F_NEXT, 1);
    write_desc(mem, desc, 1, resp, response_len, DESC_F_WRITE, 0);
    mem.write(avail + 2, &next_avail.to_le_bytes());
    mem.write(avail + 4 + u64::from(ring_slot) * 2, &0u16.to_le_bytes());
    pci_write(dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, mem);
    assert_eq!(
        u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
        next_avail
    );
    mem.read(resp, response_len as usize)
}

pub(super) fn submit_control_readable_descs(
    dev: &mut VirtioPciGpu,
    mem: &mut TestMem,
    readable: &[&[u8]],
    response_len: u32,
) -> (Vec<u8>, u16) {
    submit_control_readable_descs_at(
        dev,
        mem,
        readable,
        response_len,
        0x4000_1000,
        0x4000_4000,
        0x4000_9000,
    )
}

pub(super) fn submit_control_readable_descs_at(
    dev: &mut VirtioPciGpu,
    mem: &mut TestMem,
    readable: &[&[u8]],
    response_len: u32,
    desc: u64,
    req: u64,
    resp: u64,
) -> (Vec<u8>, u16) {
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    setup_queue(dev, mem, 0, desc, avail, used, 0);
    let next_avail = dev.stats().queues[0].last_avail_idx.wrapping_add(1);
    let ring_slot = dev.stats().queues[0].last_avail_idx % 16;
    let mut addr = req;
    for (i, bytes) in readable.iter().enumerate() {
        mem.write(addr, bytes);
        let next = (i + 1) as u16;
        write_desc(
            mem,
            desc,
            i as u16,
            addr,
            bytes.len() as u32,
            DESC_F_NEXT,
            next,
        );
        addr += 0x100;
    }
    let response_index = readable.len() as u16;
    write_desc(
        mem,
        desc,
        response_index,
        resp,
        response_len,
        DESC_F_WRITE,
        0,
    );
    mem.write(avail + 2, &next_avail.to_le_bytes());
    mem.write(avail + 4 + u64::from(ring_slot) * 2, &0u16.to_le_bytes());
    pci_write(dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, mem);
    let used_idx = u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap());
    (mem.read(resp, response_len as usize), used_idx)
}

pub(super) fn ctx_create_req(ctx_id: u32, context_init: u32, name: &[u8]) -> Vec<u8> {
    let mut req = ctrl_req_ctx(VIRTIO_GPU_CMD_CTX_CREATE, ctx_id);
    req.extend_from_slice(&(name.len() as u32).to_le_bytes());
    req.extend_from_slice(&context_init.to_le_bytes());
    let mut debug_name = [0u8; 64];
    debug_name[..name.len().min(64)].copy_from_slice(&name[..name.len().min(64)]);
    req.extend_from_slice(&debug_name);
    req
}

pub(super) fn submit_3d_req(ctx_id: u32, cmdbuf: &[u8]) -> Vec<u8> {
    let mut req = ctrl_req_ctx(VIRTIO_GPU_CMD_SUBMIT_3D, ctx_id);
    req.extend_from_slice(&(cmdbuf.len() as u32).to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(cmdbuf);
    req
}

pub(super) fn program_config_msix_vector(dev: &mut VirtioPciGpu, vector: u16) {
    let mut mem = TestMem::new(0x4000_0000, 0x1000);
    assert_eq!(
        dev.access(
            PCI_COMMON_CFG_OFFSET + COMMON_CONFIG_MSIX_VECTOR,
            VirtioPciGpuOp::Write {
                size: 2,
                value: u64::from(vector),
            },
            &mut mem,
        ),
        VirtioGpuResult::WriteAck
    );
}

pub(super) fn deferred_scanout_dev() -> (VirtioPciGpu, Arc<Mutex<MockBackend>>, TestMem) {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
    for field in [
        31u32,
        2,
        FORMAT_B8G8R8A8_UNORM,
        0x8a,
        1280,
        800,
        1,
        1,
        0,
        1,
        0,
        0,
    ] {
        create.extend_from_slice(&field.to_le_bytes());
    }
    submit_control(&mut dev, &mut mem, &create, 24);
    let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
    for field in [0u32, 0, 1280, 800, 0, 31] {
        set_scanout.extend_from_slice(&field.to_le_bytes());
    }
    submit_control(&mut dev, &mut mem, &set_scanout, 24);
    dev.gpu.set_3d_scanout_deferred(true);
    (dev, backend, mem)
}

pub(super) fn flush_res_31() -> Vec<u8> {
    let mut flush = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_FLUSH);
    for field in [0u32, 0, 1280, 800, 31, 0] {
        flush.extend_from_slice(&field.to_le_bytes());
    }
    flush
}
