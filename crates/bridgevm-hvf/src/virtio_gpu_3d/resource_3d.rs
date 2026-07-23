//! The VirGL 3D/2D resource registry: create, backing attach/detach, transfers, scanout classification, unref.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_trace::venus_start_trace_enabled;

pub(crate) fn is_local_scanout_resource(args: Create3dArgs) -> bool {
    let display_binds = VIRGL_BIND_DISPLAY_TARGET | VIRGL_BIND_SCANOUT;
    args.target == PIPE_TEXTURE_2D
        && (1..=4).contains(&args.format)
        && args.bind & display_binds == display_binds
        && args.width > 0
        && args.height > 0
        && args.width <= MAX_LOCAL_SCANOUT_DIMENSION
        && args.height <= MAX_LOCAL_SCANOUT_DIMENSION
        && args.depth == 1
        && args.array_size == 1
        && args.last_level == 0
        && args.nr_samples == 0
}

pub(crate) const PIPE_TEXTURE_2D: u32 = 2;

pub(crate) const VIRGL_BIND_DISPLAY_TARGET: u32 = 1 << 7;

pub(crate) const VIRGL_BIND_SCANOUT: u32 = 1 << 18;

pub(crate) const MAX_LOCAL_SCANOUT_DIMENSION: u32 = 16_384;

impl VirtioGpu3d {
    pub fn unref_resource(&mut self, resource_id: u32) {
        self.resource_2d_ids.remove(&resource_id);
        let mut destroy_backend_resource = self.resource_3d_ids.remove(&resource_id);
        if self.local_3d_backing.remove(&resource_id).is_some() {
            destroy_backend_resource = false;
        }
        self.resource_3d_info.remove(&resource_id);
        if let Some(resource) = self.blob_resources.get(&resource_id) {
            if resource.mapped.is_some() {
                self.destroyed_blob_mapped_ids.insert(resource_id);
                self.destroyed_blob_unmapped_ids.remove(&resource_id);
            } else {
                self.destroyed_blob_unmapped_ids.insert(resource_id);
                self.destroyed_blob_mapped_ids.remove(&resource_id);
            }
            self.unmap_blob_resource(resource_id);
            self.blob_resources.remove(&resource_id);
            self.mapped_intervals
                .retain(|_, (_, mapped_resource)| *mapped_resource != resource_id);
            destroy_backend_resource = true;
        }
        if destroy_backend_resource {
            if let Some(backend) = self.backend.as_mut() {
                backend.destroy_resource(resource_id);
            }
        }
    }

    pub fn register_2d_resource(&mut self, resource_id: u32) {
        if resource_id != 0 {
            self.resource_2d_ids.insert(resource_id);
        }
    }

    pub fn is_3d_resource(&self, resource_id: u32) -> bool {
        self.resource_3d_ids.contains(&resource_id)
    }

    pub fn scanout_3d_info(&self, resource_id: u32) -> Option<Create3dArgs> {
        self.resource_3d_info.get(&resource_id).copied()
    }

    pub fn local_3d_backing(&self, resource_id: u32) -> Option<&[BlobMemEntry]> {
        self.local_3d_backing.get(&resource_id).map(Vec::as_slice)
    }

    pub fn attach_3d_backing(
        &mut self,
        mem: &dyn GuestMemoryMut,
        resource_id: u32,
        backing: &[BlobMemEntry],
    ) -> bool {
        if !self.resource_3d_ids.contains(&resource_id) || backing.is_empty() {
            return false;
        }
        self.host_iovecs_scratch.clear();
        if !resolve_blob_iovecs_into(mem, backing, &mut self.host_iovecs_scratch) {
            return false;
        }
        if let Some(local_backing) = self.local_3d_backing.get_mut(&resource_id) {
            let Some(info) = self.resource_3d_info.get(&resource_id) else {
                self.host_iovecs_scratch.clear();
                return false;
            };
            let required = u64::from(info.width)
                .checked_mul(u64::from(info.height))
                .and_then(|pixels| pixels.checked_mul(4));
            let available = backing.iter().fold(0u64, |total, entry| {
                total.saturating_add(u64::from(entry.len))
            });
            if !matches!(required, Some(required) if available >= required) {
                self.host_iovecs_scratch.clear();
                return false;
            }
            local_backing.clear();
            local_backing.extend_from_slice(backing);
            self.host_iovecs_scratch.clear();
            return true;
        }
        let attached = self
            .backend
            .as_mut()
            .is_some_and(|backend| backend.attach_backing(resource_id, &self.host_iovecs_scratch));
        self.host_iovecs_scratch.clear();
        attached
    }

    pub fn detach_3d_backing(&mut self, resource_id: u32) -> bool {
        if let Some(backing) = self.local_3d_backing.get_mut(&resource_id) {
            backing.clear();
            return true;
        }
        self.resource_3d_ids.contains(&resource_id)
            && self
                .backend
                .as_mut()
                .is_some_and(|backend| backend.detach_backing(resource_id))
    }

    pub(crate) fn resource_create_3d_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if request.len() < RESOURCE_CREATE_3D_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let args = Create3dArgs {
            resource_id: read_le_u32(request, 24).unwrap_or(0),
            target: read_le_u32(request, 28).unwrap_or(0),
            format: read_le_u32(request, 32).unwrap_or(0),
            bind: read_le_u32(request, 36).unwrap_or(0),
            width: read_le_u32(request, 40).unwrap_or(0),
            height: read_le_u32(request, 44).unwrap_or(0),
            depth: read_le_u32(request, 48).unwrap_or(0),
            array_size: read_le_u32(request, 52).unwrap_or(0),
            last_level: read_le_u32(request, 56).unwrap_or(0),
            nr_samples: read_le_u32(request, 60).unwrap_or(0),
            flags: read_le_u32(request, 64).unwrap_or(0),
        };
        if args.resource_id == 0 || self.resource_exists(args.resource_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        // The Venus WDDM KMD creates its shared primary before the UMD has
        // created the context whose numeric id is used by the subsequent
        // CTX_ATTACH_RESOURCE.  Keep that narrowly identified display resource
        // in guest backing even when the renderer also supports legacy virgl
        // resources; otherwise the early attach is lost inside virglrenderer.
        // Non-scanout render targets continue through the renderer below.
        let local_scanout = self.backend.is_some() && is_local_scanout_resource(args);
        let created = local_scanout
            || self
                .backend
                .as_mut()
                .is_some_and(|backend| backend.create_3d(args));
        if !created {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
            return;
        }
        self.resource_3d_ids.insert(args.resource_id);
        self.resource_3d_info.insert(args.resource_id, args);
        if local_scanout {
            self.local_3d_backing.insert(args.resource_id, Vec::new());
            if venus_start_trace_enabled() {
                println!(
                    "venus-start: local display resource_create_3d res={} format={} bind={:#x} size={}x{}",
                    args.resource_id, args.format, args.bind, args.width, args.height
                );
            }
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    pub(crate) fn transfer_3d_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        to_host: bool,
        out: &mut Vec<u8>,
    ) {
        if request.len() < TRANSFER_3D_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let args = Transfer3dArgs {
            ctx_id: hdr.ctx_id,
            resource_id: read_le_u32(request, 56).unwrap_or(0),
            x: read_le_u32(request, 24).unwrap_or(0),
            y: read_le_u32(request, 28).unwrap_or(0),
            z: read_le_u32(request, 32).unwrap_or(0),
            width: read_le_u32(request, 36).unwrap_or(0),
            height: read_le_u32(request, 40).unwrap_or(0),
            depth: read_le_u32(request, 44).unwrap_or(0),
            offset: read_le_u64(request, 48).unwrap_or(0),
            level: read_le_u32(request, 60).unwrap_or(0),
            stride: read_le_u32(request, 64).unwrap_or(0),
            layer_stride: read_le_u32(request, 68).unwrap_or(0),
        };
        if !self.resource_3d_ids.contains(&args.resource_id)
            || args.width == 0
            || args.height == 0
            || args.depth == 0
        {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let transferred = self.local_3d_backing.contains_key(&args.resource_id)
            || self
                .backend
                .as_mut()
                .is_some_and(|backend| backend.transfer_3d(args, to_host));
        response_hdr_into(
            out,
            if transferred {
                VIRTIO_GPU_RESP_OK_NODATA
            } else {
                VIRTIO_GPU_RESP_ERR_UNSPEC
            },
            Some(hdr),
        );
    }

    pub(crate) fn resource_exists(&self, resource_id: u32) -> bool {
        resource_id != 0
            && (self.resource_2d_ids.contains(&resource_id)
                || self.resource_3d_ids.contains(&resource_id)
                || self.blob_resources.contains_key(&resource_id))
    }
}
