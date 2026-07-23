use super::*;

pub(crate) fn install_gpu_shm_port(platform: &mut VirtPlatform) -> Arc<Mutex<HvGpuShmMapState>> {
    let hv_gpu_shm_state = Arc::new(Mutex::new(HvGpuShmMapState::default()));
    let installed_hv_gpu_shm_port =
        platform.set_virtio_gpu_shm_map_port(Box::new(HvGpuShmMapPort {
            state: Arc::clone(&hv_gpu_shm_state),
        }));
    if installed_hv_gpu_shm_port {
        println!("virtio-gpu host-visible shm map port: hv_vm_map enabled");
    }
    hv_gpu_shm_state
}
