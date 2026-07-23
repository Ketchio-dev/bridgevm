//! Split out of mmio.rs by responsibility.

use super::super::*;
use super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioBlockDeviceProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub register_address_registers_set: bool,
    pub device_bus_created: bool,
    pub device_bus_device_count: usize,
    pub register_reads: Vec<HvfMmioBlockRegisterProbe>,
    pub continuation_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub vendor_value_preserved: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub device_models: &'static str,
    pub code_ipa_start: u64,
    pub block_ipa: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub magic_value: u64,
    pub version_value: u64,
    pub device_id_value: u64,
    pub vendor_id_value: u64,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub register_address_registers_set_status: Vec<Option<i32>>,
    pub continuation_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub continuation_watchdog_cancel_status: Option<i32>,
    pub vendor_value_after_continue_status: Option<i32>,
    pub vendor_value_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioBlockDeviceProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO block device probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: LDR W0 VirtIO-MMIO identity registers, then HVC\n");
        output.push_str(&format!("Device models: {}\n", self.device_models));
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
            "Register address registers set: {}\n",
            self.register_address_registers_set
        ));
        output.push_str(&format!(
            "Device bus created: {}\n",
            self.device_bus_created
        ));
        output.push_str(&format!(
            "Device bus device count: {}\n",
            self.device_bus_device_count
        ));
        output.push_str("VirtIO-MMIO block identity reads:\n");
        for read in &self.register_reads {
            output.push_str(&format!(
                "- {} at {:#x}: expected {:#x}, run={}, exit={}, handled={}, injected={}, pc_advanced={}\n",
                read.name,
                read.ipa,
                read.expected_value,
                read.run_attempted,
                read.exit_observed,
                read.handled_by_device,
                read.value_injected,
                read.pc_advanced
            ));
            output.push_str(&format!(
                "  {} run status name: {}\n",
                read.name,
                render_optional_status_name(read.run_status)
            ));
            output.push_str(&format!(
                "  {} exit reason name: {}\n",
                read.name,
                render_optional_exit_reason_name(read.exit_reason)
            ));
            output.push_str(&format!(
                "  {} exit syndrome: {}\n",
                read.name,
                render_optional_u64(read.exit_syndrome)
            ));
            output.push_str(&format!(
                "  {} exit virtual address: {}\n",
                read.name,
                render_optional_u64(read.exit_virtual_address)
            ));
            output.push_str(&format!(
                "  {} exit physical address: {}\n",
                read.name,
                render_optional_u64(read.exit_physical_address)
            ));
            output.push_str(&format!(
                "  {} watchdog cancel status name: {}\n",
                read.name,
                render_optional_status_name(read.watchdog_cancel_status)
            ));
            output.push_str(&format!(
                "  {} value set status name: {}\n",
                read.name,
                render_optional_status_name(read.value_set_status)
            ));
            output.push_str(&format!(
                "  {} PC read status name: {}\n",
                read.name,
                render_optional_status_name(read.pc_read_status)
            ));
            output.push_str(&format!(
                "  {} PC after exit: {}\n",
                read.name,
                render_optional_u64(read.pc_after_exit)
            ));
            output.push_str(&format!(
                "  {} PC advance status name: {}\n",
                read.name,
                render_optional_status_name(read.pc_advance_status)
            ));
        }
        output.push_str(&format!(
            "Continuation run attempted: {}\n",
            self.continuation_run_attempted
        ));
        output.push_str(&format!(
            "Continuation exit observed: {}\n",
            self.continuation_exit_observed
        ));
        output.push_str(&format!(
            "Vendor value preserved: {}\n",
            self.vendor_value_preserved
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
        output.push_str(&format!("Block IPA: {:#x}\n", self.block_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!("VirtIO magic value: {:#x}\n", self.magic_value));
        output.push_str(&format!(
            "VirtIO version value: {:#x}\n",
            self.version_value
        ));
        output.push_str(&format!(
            "VirtIO block device ID value: {:#x}\n",
            self.device_id_value
        ));
        output.push_str(&format!(
            "VirtIO vendor ID value: {:#x}\n",
            self.vendor_id_value
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
        output.push_str("Register address set status names:\n");
        for (index, status) in self
            .register_address_registers_set_status
            .iter()
            .enumerate()
        {
            output.push_str(&format!(
                "- X{}: {}\n",
                index + 1,
                render_optional_status_name(*status)
            ));
        }
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
            "Vendor value after continue status name: {}\n",
            render_optional_status_name(self.vendor_value_after_continue_status)
        ));
        output.push_str(&format!(
            "Vendor value after continue: {}\n",
            render_optional_u64(self.vendor_value_after_continue)
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

pub fn probe_hvf_mmio_block_device(allow_device: bool) -> HvfMmioBlockDeviceProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_block_device(allow_device, host)
}
