#[cfg(not(feature = "venus"))]
fn main() {
    eprintln!("venus_device_smoke built without venus feature");
}

#[cfg(feature = "venus")]
mod smoke {
    use std::{
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    use bridgevm_hvf::{
        fwcfg::GuestMemoryMut,
        venus_backend::VenusBackend,
        virtio_gpu::{VirtioGpuResult, VirtioPciGpu, VirtioPciGpuOp},
        virtio_gpu_3d::{
            GpuShmMapPort, VIRTIO_GPU_BLOB_MEM_HOST3D, VIRTIO_GPU_CMD_CTX_CREATE,
            VIRTIO_GPU_CMD_CTX_DESTROY, VIRTIO_GPU_CMD_GET_CAPSET, VIRTIO_GPU_CMD_GET_CAPSET_INFO,
            VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB, VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB,
            VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB, VIRTIO_GPU_CMD_SUBMIT_3D, VIRTIO_GPU_FLAG_FENCE,
            VIRTIO_GPU_FLAG_INFO_RING_IDX, VIRTIO_GPU_RESP_OK_CAPSET,
            VIRTIO_GPU_RESP_OK_CAPSET_INFO, VIRTIO_GPU_RESP_OK_MAP_INFO, VIRTIO_GPU_RESP_OK_NODATA,
        },
    };

    const COMMON_QUEUE_SELECT: u64 = 0x16;
    const COMMON_QUEUE_SIZE: u64 = 0x18;
    const COMMON_QUEUE_ENABLE: u64 = 0x1c;
    const COMMON_QUEUE_DESC: u64 = 0x20;
    const COMMON_QUEUE_DRIVER: u64 = 0x28;
    const COMMON_QUEUE_DEVICE: u64 = 0x30;
    const PCI_NOTIFY_CFG_OFFSET: u64 = 0x3000;
    const DESC_SIZE: u64 = 16;
    const DESC_F_NEXT: u16 = 1;
    const DESC_F_WRITE: u16 = 2;
    const VIRTIO_GPU_CMD_RESOURCE_UNREF: u32 = 0x0102;
    const VIRTIO_GPU_BLOB_FLAG_USE_MAPPABLE: u32 = 1;
    const SHM_WINDOW_SIZE: u64 = 1024 * 1024 * 1024;
    const HVF_PAGE_SIZE: usize = 16 * 1024;

    #[derive(Debug)]
    struct TestMem {
        base: u64,
        bytes: Vec<u8>,
    }

    impl TestMem {
        fn new(base: u64, len: usize) -> Self {
            Self {
                base,
                bytes: vec![0; len],
            }
        }

        fn write(&mut self, gpa: u64, data: &[u8]) {
            assert!(self.write_bytes(gpa, data));
        }

        fn read(&self, gpa: u64, len: usize) -> Vec<u8> {
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
    }

    #[derive(Debug, Default)]
    struct RecordingMapPort {
        maps: Vec<(usize, usize, u64)>,
        unmaps: Vec<(u64, usize)>,
    }

    #[derive(Debug, Clone)]
    struct RecordingMapPortHandle(Arc<Mutex<RecordingMapPort>>);

    impl GpuShmMapPort for RecordingMapPortHandle {
        fn map(&mut self, host_ptr: *mut u8, size: usize, shm_offset: u64) -> Result<(), i32> {
            assert!(!host_ptr.is_null());
            assert_eq!((host_ptr as usize) % HVF_PAGE_SIZE, 0);
            assert!(size >= 65_536);
            assert_eq!(size % HVF_PAGE_SIZE, 0);
            unsafe {
                let old = host_ptr.read();
                host_ptr.write(old.wrapping_add(1));
                assert_eq!(host_ptr.read(), old.wrapping_add(1));
                host_ptr.write(old);
            }
            self.0
                .lock()
                .unwrap()
                .maps
                .push((host_ptr as usize, size, shm_offset));
            Ok(())
        }

        fn unmap(&mut self, shm_offset: u64, size: usize) -> Result<(), i32> {
            self.0.lock().unwrap().unmaps.push((shm_offset, size));
            Ok(())
        }
    }

    pub(super) fn main() {
        let backend = VenusBackend::new().expect("VenusBackend init");
        let map_port = Arc::new(Mutex::new(RecordingMapPort::default()));
        let mut dev = VirtioPciGpu::with_3d_backend_and_shm_map_port(
            1280,
            800,
            Box::new(backend),
            Box::new(RecordingMapPortHandle(map_port.clone())),
            SHM_WINDOW_SIZE,
        );
        let mut mem = TestMem::new(0x4000_0000, 0x40000);

        let mut info = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET_INFO, 0);
        info.extend_from_slice(&0u32.to_le_bytes());
        info.extend_from_slice(&0u32.to_le_bytes());
        let resp = submit_control(&mut dev, &mut mem, &info, 40);
        assert_eq!(read_u32(&resp, 0), VIRTIO_GPU_RESP_OK_CAPSET_INFO);
        assert_eq!(read_u32(&resp, 24), 4);
        let max_version = read_u32(&resp, 28);
        assert_eq!(read_u32(&resp, 32), 160);

        let mut get = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET, 0);
        get.extend_from_slice(&4u32.to_le_bytes());
        get.extend_from_slice(&max_version.to_le_bytes());
        let resp = submit_control(&mut dev, &mut mem, &get, 24 + 160);
        assert_eq!(read_u32(&resp, 0), VIRTIO_GPU_RESP_OK_CAPSET);
        assert_eq!(read_u32(&resp, 24), 1);

        let resp = submit_control(
            &mut dev,
            &mut mem,
            &ctx_create_req(1, 4, b"venus-device-smoke"),
            24,
        );
        assert_eq!(read_u32(&resp, 0), VIRTIO_GPU_RESP_OK_NODATA);

        let resp = submit_control(&mut dev, &mut mem, &submit_3d_req(1, &[]), 24);
        println!("empty submit_3d response={:#x}", read_u32(&resp, 0));

        wait_for_fenced_submit(&mut dev, &mut mem, 1, 0, 99, "ring0 fence");
        assert_rejected_fenced_submit_completes(&mut dev, &mut mem, 1, 3, 100, "ring3 fence");
        println!("VENUS_FENCE_OK ring0=retired invalid_ring=completed");

        let resp = submit_control(
            &mut dev,
            &mut mem,
            &create_blob_req(
                11,
                VIRTIO_GPU_BLOB_MEM_HOST3D,
                VIRTIO_GPU_BLOB_FLAG_USE_MAPPABLE,
                0,
                65_536,
                1,
            ),
            24,
        );
        assert_eq!(read_u32(&resp, 0), VIRTIO_GPU_RESP_OK_NODATA);

        let resp = submit_control(&mut dev, &mut mem, &map_blob_req(11, 0), 32);
        assert_eq!(read_u32(&resp, 0), VIRTIO_GPU_RESP_OK_MAP_INFO);
        let map_info = read_u32(&resp, 24);
        let (ptr_aligned, mapped_size) = {
            let guard = map_port.lock().unwrap();
            let (ptr, size, offset) = guard.maps.last().copied().expect("blob map call");
            assert_eq!(offset, 0);
            (usize::from(ptr % HVF_PAGE_SIZE == 0), size)
        };

        let resp = submit_control(&mut dev, &mut mem, &unmap_blob_req(11), 24);
        assert_eq!(read_u32(&resp, 0), VIRTIO_GPU_RESP_OK_NODATA);

        let resp = submit_control(&mut dev, &mut mem, &resource_unref_req(11), 24);
        assert_eq!(read_u32(&resp, 0), VIRTIO_GPU_RESP_OK_NODATA);
        println!(
            "VENUS_BLOB_OK ptr_aligned={} size={} map_info={:#x}",
            ptr_aligned, mapped_size, map_info
        );

        let resp = submit_control(
            &mut dev,
            &mut mem,
            &ctrl_req(VIRTIO_GPU_CMD_CTX_DESTROY, 1),
            24,
        );
        assert_eq!(read_u32(&resp, 0), VIRTIO_GPU_RESP_OK_NODATA);
        println!(
            "PASS: venus_device_smoke capset_size=160 fenced_used_idx={}",
            used_idx(&mem)
        );
    }

    fn pci_write(dev: &mut VirtioPciGpu, offset: u64, size: u8, value: u64, mem: &mut TestMem) {
        assert_eq!(
            dev.access(offset, VirtioPciGpuOp::Write { size, value }, mem),
            VirtioGpuResult::WriteAck
        );
    }

    fn setup_queue(dev: &mut VirtioPciGpu, mem: &mut TestMem) {
        pci_write(dev, COMMON_QUEUE_SELECT, 2, 0, mem);
        pci_write(dev, COMMON_QUEUE_SIZE, 2, 16, mem);
        pci_write(dev, COMMON_QUEUE_DESC, 8, 0x4000_1000, mem);
        pci_write(dev, COMMON_QUEUE_DRIVER, 8, 0x4000_2000, mem);
        pci_write(dev, COMMON_QUEUE_DEVICE, 8, 0x4000_3000, mem);
        pci_write(dev, COMMON_QUEUE_ENABLE, 2, 1, mem);
    }

    fn write_desc(mem: &mut TestMem, index: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let gpa = 0x4000_1000 + u64::from(index) * DESC_SIZE;
        mem.write(gpa, &addr.to_le_bytes());
        mem.write(gpa + 8, &len.to_le_bytes());
        mem.write(gpa + 12, &flags.to_le_bytes());
        mem.write(gpa + 14, &next.to_le_bytes());
    }

    fn submit_control(
        dev: &mut VirtioPciGpu,
        mem: &mut TestMem,
        request: &[u8],
        response_len: u32,
    ) -> Vec<u8> {
        setup_queue(dev, mem);
        let next_avail = dev.stats().queues[0].last_avail_idx.wrapping_add(1);
        let ring_slot = dev.stats().queues[0].last_avail_idx % 16;
        mem.write(0x4000_4000, request);
        write_desc(mem, 0, 0x4000_4000, request.len() as u32, DESC_F_NEXT, 1);
        write_desc(mem, 1, 0x4000_5000, response_len, DESC_F_WRITE, 0);
        mem.write(0x4000_2000 + 2, &next_avail.to_le_bytes());
        mem.write(
            0x4000_2000 + 4 + u64::from(ring_slot) * 2,
            &0u16.to_le_bytes(),
        );
        pci_write(dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, mem);
        mem.read(0x4000_5000, response_len as usize)
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

    fn ctx_create_req(ctx_id: u32, context_init: u32, name: &[u8]) -> Vec<u8> {
        let mut req = ctrl_req(VIRTIO_GPU_CMD_CTX_CREATE, ctx_id);
        req.extend_from_slice(&(name.len() as u32).to_le_bytes());
        req.extend_from_slice(&context_init.to_le_bytes());
        let mut debug_name = [0u8; 64];
        debug_name[..name.len().min(64)].copy_from_slice(&name[..name.len().min(64)]);
        req.extend_from_slice(&debug_name);
        req
    }

    fn submit_3d_req(ctx_id: u32, cmdbuf: &[u8]) -> Vec<u8> {
        let mut req = ctrl_req(VIRTIO_GPU_CMD_SUBMIT_3D, ctx_id);
        req.extend_from_slice(&(cmdbuf.len() as u32).to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());
        req.extend_from_slice(cmdbuf);
        req
    }

    fn fenced_submit_3d_req(ctx_id: u32, ring_idx: u8, fence_id: u64, cmdbuf: &[u8]) -> Vec<u8> {
        let mut req = ctrl_req(VIRTIO_GPU_CMD_SUBMIT_3D, ctx_id);
        req[4..8].copy_from_slice(
            &(VIRTIO_GPU_FLAG_FENCE | VIRTIO_GPU_FLAG_INFO_RING_IDX).to_le_bytes(),
        );
        req[8..16].copy_from_slice(&fence_id.to_le_bytes());
        req[20] = ring_idx;
        req.extend_from_slice(&(cmdbuf.len() as u32).to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());
        req.extend_from_slice(cmdbuf);
        req
    }

    fn wait_for_fenced_submit(
        dev: &mut VirtioPciGpu,
        mem: &mut TestMem,
        ctx_id: u32,
        ring_idx: u8,
        fence_id: u64,
        label: &str,
    ) {
        let before = used_idx(mem);
        let req = fenced_submit_3d_req(ctx_id, ring_idx, fence_id, &[]);
        let _ = submit_control(dev, mem, &req, 24);
        assert_eq!(
            used_idx(mem),
            before,
            "{label} completed before async fence callback"
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while used_idx(mem) == before {
            dev.drain_completed_fences(mem);
            if Instant::now() > deadline {
                panic!("timed out waiting for venus {label}");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn assert_rejected_fenced_submit_completes(
        dev: &mut VirtioPciGpu,
        mem: &mut TestMem,
        ctx_id: u32,
        ring_idx: u8,
        fence_id: u64,
        label: &str,
    ) {
        let before = used_idx(mem);
        let req = fenced_submit_3d_req(ctx_id, ring_idx, fence_id, &[]);
        let resp = submit_control(dev, mem, &req, 24);
        assert_eq!(read_u32(&resp, 0), VIRTIO_GPU_RESP_OK_NODATA);
        assert_eq!(
            used_idx(mem),
            before.wrapping_add(1),
            "{label} did not complete immediately"
        );
    }

    fn create_blob_req(
        resource_id: u32,
        blob_mem: u32,
        blob_flags: u32,
        blob_id: u64,
        size: u64,
        ctx_id: u32,
    ) -> Vec<u8> {
        let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB, ctx_id);
        req.extend_from_slice(&resource_id.to_le_bytes());
        req.extend_from_slice(&blob_mem.to_le_bytes());
        req.extend_from_slice(&blob_flags.to_le_bytes());
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

    fn resource_unref_req(resource_id: u32) -> Vec<u8> {
        let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_UNREF, 0);
        req.extend_from_slice(&resource_id.to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());
        req
    }

    fn read_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn used_idx(mem: &TestMem) -> u16 {
        u16::from_le_bytes(mem.read(0x4000_3000 + 2, 2).try_into().unwrap())
    }
}

#[cfg(feature = "venus")]
fn main() {
    smoke::main();
}
