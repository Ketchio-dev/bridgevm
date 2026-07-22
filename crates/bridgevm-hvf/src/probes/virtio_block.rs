//! VirtIO block probes: request model, and file / writable-file / ISO backings.
//!
//! Moved verbatim out of the legacy probe monolith. Items keep the visibility
//! they had at the crate root and are re-exported there, so the public API is
//! unchanged. The live backends live in `crate::platform`.

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtioBlockRequestModelProbe {
    pub configured_via_mmio: bool,
    pub configured_via_mmio_bus: bool,
    pub queue_notified: bool,
    pub queue_notify_value: Option<u64>,
    pub completed_via_device_bus: bool,
    pub completed: bool,
    pub descriptor_index: Option<u16>,
    pub request_type: Option<u32>,
    pub sector: Option<u64>,
    pub data_bytes: Option<u32>,
    pub data_prefix: Vec<u8>,
    pub status: Option<u8>,
    pub used_index: Option<u16>,
    pub used_len: Option<u32>,
    pub interrupt_status: Option<u64>,
    pub blockers: Vec<String>,
}

impl VirtioBlockRequestModelProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("VirtIO block request model probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str("Guest execution: not entered; in-memory VirtIO block descriptor chain\n");
        output.push_str(&format!(
            "Configured via MMIO: {}\n",
            self.configured_via_mmio
        ));
        output.push_str(&format!(
            "Configured via MMIO bus: {}\n",
            self.configured_via_mmio_bus
        ));
        output.push_str(&format!("Queue notified: {}\n", self.queue_notified));
        output.push_str(&format!(
            "Queue notify value: {}\n",
            render_optional_u64(self.queue_notify_value)
        ));
        output.push_str(&format!(
            "Completed via device bus: {}\n",
            self.completed_via_device_bus
        ));
        output.push_str(&format!("Completed: {}\n", self.completed));
        output.push_str(&format!(
            "Descriptor index: {}\n",
            render_optional_u64(self.descriptor_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Request type: {}\n",
            render_optional_u64(self.request_type.map(u64::from))
        ));
        output.push_str(&format!("Sector: {}\n", render_optional_u64(self.sector)));
        output.push_str(&format!(
            "Data bytes: {}\n",
            render_optional_u64(self.data_bytes.map(u64::from))
        ));
        output.push_str(&format!(
            "Data prefix: {}\n",
            render_hex_bytes(&self.data_prefix)
        ));
        output.push_str(&format!(
            "Status byte: {}\n",
            render_optional_u64(self.status.map(u64::from))
        ));
        output.push_str(&format!(
            "Used index: {}\n",
            render_optional_u64(self.used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Used length: {}\n",
            render_optional_u64(self.used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Interrupt status: {}\n",
            render_optional_u64(self.interrupt_status)
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

pub fn probe_virtio_block_request_model() -> VirtioBlockRequestModelProbe {
    match run_virtio_block_request_model() {
        Ok(probe) => probe,
        Err(error) => VirtioBlockRequestModelProbe {
            configured_via_mmio: false,
            configured_via_mmio_bus: false,
            queue_notified: false,
            queue_notify_value: None,
            completed_via_device_bus: false,
            completed: false,
            descriptor_index: None,
            request_type: None,
            sector: None,
            data_bytes: None,
            data_prefix: Vec::new(),
            status: None,
            used_index: None,
            used_len: None,
            interrupt_status: None,
            blockers: vec![error.render_blocker()],
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtioBlockFileBackingProbe {
    pub disk_path: PathBuf,
    pub backing_kind: &'static str,
    pub configured_via_mmio: bool,
    pub configured_via_mmio_bus: bool,
    pub queue_notified: bool,
    pub queue_notify_value: Option<u64>,
    pub completed_via_device_bus: bool,
    pub completed: bool,
    pub descriptor_index: Option<u16>,
    pub request_type: Option<u32>,
    pub sector: Option<u64>,
    pub byte_offset: Option<u64>,
    pub data_bytes: Option<u32>,
    pub data_prefix: Vec<u8>,
    pub status: Option<u8>,
    pub used_index: Option<u16>,
    pub used_len: Option<u32>,
    pub interrupt_status: Option<u64>,
    pub blockers: Vec<String>,
}

impl VirtioBlockFileBackingProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("VirtIO block file backing probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str(
            "Guest execution: not entered; host file-backed VirtIO block descriptor chain\n",
        );
        output.push_str(&format!("Disk path: {}\n", self.disk_path.display()));
        output.push_str(&format!("Backing kind: {}\n", self.backing_kind));
        output.push_str(&format!(
            "Configured via MMIO: {}\n",
            self.configured_via_mmio
        ));
        output.push_str(&format!(
            "Configured via MMIO bus: {}\n",
            self.configured_via_mmio_bus
        ));
        output.push_str(&format!("Queue notified: {}\n", self.queue_notified));
        output.push_str(&format!(
            "Queue notify value: {}\n",
            render_optional_u64(self.queue_notify_value)
        ));
        output.push_str(&format!(
            "Completed via device bus: {}\n",
            self.completed_via_device_bus
        ));
        output.push_str(&format!("Completed: {}\n", self.completed));
        output.push_str(&format!(
            "Descriptor index: {}\n",
            render_optional_u64(self.descriptor_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Request type: {}\n",
            render_optional_u64(self.request_type.map(u64::from))
        ));
        output.push_str(&format!("Sector: {}\n", render_optional_u64(self.sector)));
        output.push_str(&format!(
            "Byte offset: {}\n",
            render_optional_u64(self.byte_offset)
        ));
        output.push_str(&format!(
            "Data bytes: {}\n",
            render_optional_u64(self.data_bytes.map(u64::from))
        ));
        output.push_str(&format!(
            "Data prefix: {}\n",
            render_hex_bytes(&self.data_prefix)
        ));
        output.push_str(&format!(
            "Status byte: {}\n",
            render_optional_u64(self.status.map(u64::from))
        ));
        output.push_str(&format!(
            "Used index: {}\n",
            render_optional_u64(self.used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Used length: {}\n",
            render_optional_u64(self.used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Interrupt status: {}\n",
            render_optional_u64(self.interrupt_status)
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

pub fn probe_virtio_block_file_backing(disk_path: PathBuf) -> VirtioBlockFileBackingProbe {
    match run_virtio_block_file_backing(disk_path.clone()) {
        Ok(probe) => probe,
        Err(error) => VirtioBlockFileBackingProbe {
            disk_path,
            backing_kind: "host-file",
            configured_via_mmio: false,
            configured_via_mmio_bus: false,
            queue_notified: false,
            queue_notify_value: None,
            completed_via_device_bus: false,
            completed: false,
            descriptor_index: None,
            request_type: None,
            sector: None,
            byte_offset: None,
            data_bytes: None,
            data_prefix: Vec::new(),
            status: None,
            used_index: None,
            used_len: None,
            interrupt_status: None,
            blockers: vec![error.render_blocker()],
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtioBlockWritableFileBackingProbe {
    pub disk_path: PathBuf,
    pub backing_kind: &'static str,
    pub configured_via_mmio: bool,
    pub configured_via_mmio_bus: bool,
    pub queue_notified: bool,
    pub queue_notify_value: Option<u64>,
    pub initial_read_prefix: Vec<u8>,
    pub write_completed: bool,
    pub write_request_type: Option<u32>,
    pub write_sector: Option<u64>,
    pub write_byte_offset: Option<u64>,
    pub write_data_bytes: Option<u32>,
    pub write_data_prefix: Vec<u8>,
    pub write_status: Option<u8>,
    pub write_used_index: Option<u16>,
    pub write_used_len: Option<u32>,
    pub flush_completed: bool,
    pub flush_request_type: Option<u32>,
    pub flush_status: Option<u8>,
    pub flush_used_index: Option<u16>,
    pub flush_used_len: Option<u32>,
    pub persisted_data_prefix: Vec<u8>,
    pub interrupt_status: Option<u64>,
    pub blockers: Vec<String>,
}

impl VirtioBlockWritableFileBackingProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("VirtIO block writable file backing probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str(
            "Guest execution: not entered; host file-backed VirtIO block write/flush persistence descriptor chain\n",
        );
        output.push_str(&format!("Disk path: {}\n", self.disk_path.display()));
        output.push_str(&format!("Backing kind: {}\n", self.backing_kind));
        output.push_str(&format!(
            "Configured via MMIO: {}\n",
            self.configured_via_mmio
        ));
        output.push_str(&format!(
            "Configured via MMIO bus: {}\n",
            self.configured_via_mmio_bus
        ));
        output.push_str(&format!("Queue notified: {}\n", self.queue_notified));
        output.push_str(&format!(
            "Queue notify value: {}\n",
            render_optional_u64(self.queue_notify_value)
        ));
        output.push_str(&format!(
            "Initial read data prefix: {}\n",
            render_hex_bytes(&self.initial_read_prefix)
        ));
        output.push_str(&format!("Write completed: {}\n", self.write_completed));
        output.push_str(&format!(
            "Write request type: {}\n",
            render_optional_u64(self.write_request_type.map(u64::from))
        ));
        output.push_str(&format!(
            "Write sector: {}\n",
            render_optional_u64(self.write_sector)
        ));
        output.push_str(&format!(
            "Write byte offset: {}\n",
            render_optional_u64(self.write_byte_offset)
        ));
        output.push_str(&format!(
            "Write data bytes: {}\n",
            render_optional_u64(self.write_data_bytes.map(u64::from))
        ));
        output.push_str(&format!(
            "Write data prefix: {}\n",
            render_hex_bytes(&self.write_data_prefix)
        ));
        output.push_str(&format!(
            "Write status byte: {}\n",
            render_optional_u64(self.write_status.map(u64::from))
        ));
        output.push_str(&format!(
            "Write used index: {}\n",
            render_optional_u64(self.write_used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Write used length: {}\n",
            render_optional_u64(self.write_used_len.map(u64::from))
        ));
        output.push_str(&format!("Flush completed: {}\n", self.flush_completed));
        output.push_str(&format!(
            "Flush request type: {}\n",
            render_optional_u64(self.flush_request_type.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush status byte: {}\n",
            render_optional_u64(self.flush_status.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush used index: {}\n",
            render_optional_u64(self.flush_used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush used length: {}\n",
            render_optional_u64(self.flush_used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Persisted data prefix: {}\n",
            render_hex_bytes(&self.persisted_data_prefix)
        ));
        output.push_str(&format!(
            "Interrupt status: {}\n",
            render_optional_u64(self.interrupt_status)
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

pub fn probe_virtio_block_writable_file_backing(
    disk_path: PathBuf,
) -> VirtioBlockWritableFileBackingProbe {
    match run_virtio_block_writable_file_backing(disk_path.clone()) {
        Ok(probe) => probe,
        Err(error) => VirtioBlockWritableFileBackingProbe {
            disk_path,
            backing_kind: "host-file-writable",
            configured_via_mmio: false,
            configured_via_mmio_bus: false,
            queue_notified: false,
            queue_notify_value: None,
            initial_read_prefix: Vec::new(),
            write_completed: false,
            write_request_type: None,
            write_sector: None,
            write_byte_offset: None,
            write_data_bytes: None,
            write_data_prefix: Vec::new(),
            write_status: None,
            write_used_index: None,
            write_used_len: None,
            flush_completed: false,
            flush_request_type: None,
            flush_status: None,
            flush_used_index: None,
            flush_used_len: None,
            persisted_data_prefix: Vec::new(),
            interrupt_status: None,
            blockers: vec![error.render_blocker()],
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtioBlockIsoBackingProbe {
    pub iso_path: PathBuf,
    pub backing_kind: &'static str,
    pub media_mode: &'static str,
    pub configured_via_mmio: bool,
    pub configured_via_mmio_bus: bool,
    pub queue_notified: bool,
    pub queue_notify_value: Option<u64>,
    pub completed_via_device_bus: bool,
    pub completed: bool,
    pub descriptor_index: Option<u16>,
    pub request_type: Option<u32>,
    pub sector: Option<u64>,
    pub byte_offset: Option<u64>,
    pub data_bytes: Option<u32>,
    pub data_prefix: Vec<u8>,
    pub status: Option<u8>,
    pub used_index: Option<u16>,
    pub used_len: Option<u32>,
    pub interrupt_status: Option<u64>,
    pub readonly_write_rejected: bool,
    pub readonly_write_status: Option<u8>,
    pub readonly_write_used_index: Option<u16>,
    pub readonly_write_used_len: Option<u32>,
    pub readonly_write_interrupt_status: Option<u64>,
    pub blockers: Vec<String>,
}

impl VirtioBlockIsoBackingProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("VirtIO block ISO backing probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str(
            "Guest execution: not entered; read-only ISO-backed VirtIO block descriptor chain\n",
        );
        output.push_str(&format!("ISO path: {}\n", self.iso_path.display()));
        output.push_str(&format!("Backing kind: {}\n", self.backing_kind));
        output.push_str(&format!("Media mode: {}\n", self.media_mode));
        output.push_str(&format!(
            "Configured via MMIO: {}\n",
            self.configured_via_mmio
        ));
        output.push_str(&format!(
            "Configured via MMIO bus: {}\n",
            self.configured_via_mmio_bus
        ));
        output.push_str(&format!("Queue notified: {}\n", self.queue_notified));
        output.push_str(&format!(
            "Queue notify value: {}\n",
            render_optional_u64(self.queue_notify_value)
        ));
        output.push_str(&format!(
            "Completed via device bus: {}\n",
            self.completed_via_device_bus
        ));
        output.push_str(&format!("Completed: {}\n", self.completed));
        output.push_str(&format!(
            "Descriptor index: {}\n",
            render_optional_u64(self.descriptor_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Request type: {}\n",
            render_optional_u64(self.request_type.map(u64::from))
        ));
        output.push_str(&format!("Sector: {}\n", render_optional_u64(self.sector)));
        output.push_str(&format!(
            "Byte offset: {}\n",
            render_optional_u64(self.byte_offset)
        ));
        output.push_str(&format!(
            "Data bytes: {}\n",
            render_optional_u64(self.data_bytes.map(u64::from))
        ));
        output.push_str(&format!(
            "Data prefix: {}\n",
            render_hex_bytes(&self.data_prefix)
        ));
        output.push_str(&format!(
            "Status byte: {}\n",
            render_optional_u64(self.status.map(u64::from))
        ));
        output.push_str(&format!(
            "Used index: {}\n",
            render_optional_u64(self.used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Used length: {}\n",
            render_optional_u64(self.used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Interrupt status: {}\n",
            render_optional_u64(self.interrupt_status)
        ));
        output.push_str(&format!(
            "Read-only write rejected: {}\n",
            self.readonly_write_rejected
        ));
        output.push_str(&format!(
            "Read-only write status byte: {}\n",
            render_optional_u64(self.readonly_write_status.map(u64::from))
        ));
        output.push_str(&format!(
            "Read-only write used index: {}\n",
            render_optional_u64(self.readonly_write_used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Read-only write used length: {}\n",
            render_optional_u64(self.readonly_write_used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Read-only write interrupt status: {}\n",
            render_optional_u64(self.readonly_write_interrupt_status)
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

pub fn probe_virtio_block_iso_backing(iso_path: PathBuf) -> VirtioBlockIsoBackingProbe {
    match run_virtio_block_iso_backing(iso_path.clone()) {
        Ok(probe) => probe,
        Err(error) => VirtioBlockIsoBackingProbe {
            iso_path,
            backing_kind: "host-iso-readonly",
            media_mode: "read-only",
            configured_via_mmio: false,
            configured_via_mmio_bus: false,
            queue_notified: false,
            queue_notify_value: None,
            completed_via_device_bus: false,
            completed: false,
            descriptor_index: None,
            request_type: None,
            sector: None,
            byte_offset: None,
            data_bytes: None,
            data_prefix: Vec::new(),
            status: None,
            used_index: None,
            used_len: None,
            interrupt_status: None,
            readonly_write_rejected: false,
            readonly_write_status: None,
            readonly_write_used_index: None,
            readonly_write_used_len: None,
            readonly_write_interrupt_status: None,
            blockers: vec![error.render_blocker()],
        },
    }
}
