//! Split out of mmio.rs by responsibility.

use super::super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioSerialDeviceProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub write_value_register_set: bool,
    pub data_address_register_set: bool,
    pub status_address_register_set: bool,
    pub device_bus_created: bool,
    pub device_bus_device_count: usize,
    pub write_run_attempted: bool,
    pub write_exit_observed: bool,
    pub write_handled_by_device: bool,
    pub write_value_captured: bool,
    pub pc_advanced_after_write: bool,
    pub status_run_attempted: bool,
    pub status_exit_observed: bool,
    pub status_handled_by_device: bool,
    pub status_value_injected: bool,
    pub pc_advanced_after_status: bool,
    pub continuation_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub status_value_preserved: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub device_model: &'static str,
    pub code_ipa_start: u64,
    pub data_ipa: u64,
    pub status_ipa: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub serial_write_value: u64,
    pub serial_status_value: u64,
    pub captured_write_value: Option<u64>,
    pub captured_byte: Option<u8>,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub write_value_register_set_status: Option<i32>,
    pub data_address_register_set_status: Option<i32>,
    pub status_address_register_set_status: Option<i32>,
    pub write_run_status: Option<i32>,
    pub write_exit_reason: Option<u32>,
    pub write_exit_syndrome: Option<u64>,
    pub write_exit_virtual_address: Option<u64>,
    pub write_exit_physical_address: Option<u64>,
    pub write_watchdog_cancel_status: Option<i32>,
    pub write_value_capture_status: Option<i32>,
    pub pc_read_after_write_status: Option<i32>,
    pub pc_after_write_exit: Option<u64>,
    pub pc_advance_after_write_status: Option<i32>,
    pub status_run_status: Option<i32>,
    pub status_exit_reason: Option<u32>,
    pub status_exit_syndrome: Option<u64>,
    pub status_exit_virtual_address: Option<u64>,
    pub status_exit_physical_address: Option<u64>,
    pub status_watchdog_cancel_status: Option<i32>,
    pub status_value_set_status: Option<i32>,
    pub pc_read_after_status_status: Option<i32>,
    pub pc_after_status_exit: Option<u64>,
    pub pc_advance_after_status_status: Option<i32>,
    pub continuation_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub continuation_watchdog_cancel_status: Option<i32>,
    pub status_value_after_continue_status: Option<i32>,
    pub status_value_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioSerialDeviceProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO serial device probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: STR data register, LDR status register, then HVC\n");
        output.push_str(&format!("Device model: {}\n", self.device_model));
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!(
            "Write value register set: {}\n",
            self.write_value_register_set
        ));
        output.push_str(&format!(
            "Data address register set: {}\n",
            self.data_address_register_set
        ));
        output.push_str(&format!(
            "Status address register set: {}\n",
            self.status_address_register_set
        ));
        output.push_str(&format!(
            "Device bus created: {}\n",
            self.device_bus_created
        ));
        output.push_str(&format!(
            "Device bus device count: {}\n",
            self.device_bus_device_count
        ));
        output.push_str(&format!(
            "Write run attempted: {}\n",
            self.write_run_attempted
        ));
        output.push_str(&format!(
            "Write exit observed: {}\n",
            self.write_exit_observed
        ));
        output.push_str(&format!(
            "Write handled by device: {}\n",
            self.write_handled_by_device
        ));
        output.push_str(&format!(
            "Write value captured: {}\n",
            self.write_value_captured
        ));
        output.push_str(&format!(
            "PC advanced after write: {}\n",
            self.pc_advanced_after_write
        ));
        output.push_str(&format!(
            "Status run attempted: {}\n",
            self.status_run_attempted
        ));
        output.push_str(&format!(
            "Status exit observed: {}\n",
            self.status_exit_observed
        ));
        output.push_str(&format!(
            "Status handled by device: {}\n",
            self.status_handled_by_device
        ));
        output.push_str(&format!(
            "Status value injected: {}\n",
            self.status_value_injected
        ));
        output.push_str(&format!(
            "PC advanced after status: {}\n",
            self.pc_advanced_after_status
        ));
        output.push_str(&format!(
            "Continuation run attempted: {}\n",
            self.continuation_run_attempted
        ));
        output.push_str(&format!(
            "Continuation exit observed: {}\n",
            self.continuation_exit_observed
        ));
        output.push_str(&format!(
            "Status value preserved: {}\n",
            self.status_value_preserved
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("Code IPA start: {:#x}\n", self.code_ipa_start));
        output.push_str(&format!("Serial data IPA: {:#x}\n", self.data_ipa));
        output.push_str(&format!("Serial status IPA: {:#x}\n", self.status_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!(
            "Serial write value: {:#x}\n",
            self.serial_write_value
        ));
        output.push_str(&format!(
            "Serial status value: {:#x}\n",
            self.serial_status_value
        ));
        output.push_str(&format!(
            "Captured write value: {}\n",
            render_optional_u64(self.captured_write_value)
        ));
        output.push_str(&format!(
            "Captured byte: {}\n",
            self.captured_byte
                .map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
        ));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "Write value register set status name: {}\n",
            render_optional_status_name(self.write_value_register_set_status)
        ));
        output.push_str(&format!(
            "Data address register set status name: {}\n",
            render_optional_status_name(self.data_address_register_set_status)
        ));
        output.push_str(&format!(
            "Status address register set status name: {}\n",
            render_optional_status_name(self.status_address_register_set_status)
        ));
        output.push_str(&format!(
            "Write run status name: {}\n",
            render_optional_status_name(self.write_run_status)
        ));
        output.push_str(&format!(
            "Write exit reason name: {}\n",
            render_optional_exit_reason_name(self.write_exit_reason)
        ));
        output.push_str(&format!(
            "Write exit syndrome: {}\n",
            render_optional_u64(self.write_exit_syndrome)
        ));
        output.push_str(&format!(
            "Write exit virtual address: {}\n",
            render_optional_u64(self.write_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Write exit physical address: {}\n",
            render_optional_u64(self.write_exit_physical_address)
        ));
        output.push_str(&format!(
            "Write watchdog cancel status name: {}\n",
            render_optional_status_name(self.write_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Write value capture status name: {}\n",
            render_optional_status_name(self.write_value_capture_status)
        ));
        output.push_str(&format!(
            "PC read after write status name: {}\n",
            render_optional_status_name(self.pc_read_after_write_status)
        ));
        output.push_str(&format!(
            "PC after write exit: {}\n",
            render_optional_u64(self.pc_after_write_exit)
        ));
        output.push_str(&format!(
            "PC advance after write status name: {}\n",
            render_optional_status_name(self.pc_advance_after_write_status)
        ));
        output.push_str(&format!(
            "Status run status name: {}\n",
            render_optional_status_name(self.status_run_status)
        ));
        output.push_str(&format!(
            "Status exit reason name: {}\n",
            render_optional_exit_reason_name(self.status_exit_reason)
        ));
        output.push_str(&format!(
            "Status exit syndrome: {}\n",
            render_optional_u64(self.status_exit_syndrome)
        ));
        output.push_str(&format!(
            "Status exit virtual address: {}\n",
            render_optional_u64(self.status_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Status exit physical address: {}\n",
            render_optional_u64(self.status_exit_physical_address)
        ));
        output.push_str(&format!(
            "Status watchdog cancel status name: {}\n",
            render_optional_status_name(self.status_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Status value set status name: {}\n",
            render_optional_status_name(self.status_value_set_status)
        ));
        output.push_str(&format!(
            "PC read after status status name: {}\n",
            render_optional_status_name(self.pc_read_after_status_status)
        ));
        output.push_str(&format!(
            "PC after status exit: {}\n",
            render_optional_u64(self.pc_after_status_exit)
        ));
        output.push_str(&format!(
            "PC advance after status status name: {}\n",
            render_optional_status_name(self.pc_advance_after_status_status)
        ));
        output.push_str(&format!(
            "Continuation run status name: {}\n",
            render_optional_status_name(self.continuation_run_status)
        ));
        output.push_str(&format!(
            "Continuation exit reason name: {}\n",
            render_optional_exit_reason_name(self.continuation_exit_reason)
        ));
        output.push_str(&format!(
            "Continuation exit syndrome: {}\n",
            render_optional_u64(self.continuation_exit_syndrome)
        ));
        output.push_str(&format!(
            "Continuation exit virtual address: {}\n",
            render_optional_u64(self.continuation_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Continuation exit physical address: {}\n",
            render_optional_u64(self.continuation_exit_physical_address)
        ));
        output.push_str(&format!(
            "Continuation watchdog cancel status name: {}\n",
            render_optional_status_name(self.continuation_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Status value after continue status name: {}\n",
            render_optional_status_name(self.status_value_after_continue_status)
        ));
        output.push_str(&format!(
            "Status value after continue: {}\n",
            render_optional_u64(self.status_value_after_continue)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
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

pub fn probe_hvf_mmio_serial_device(allow_device: bool) -> HvfMmioSerialDeviceProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_serial_device(allow_device, host)
}
