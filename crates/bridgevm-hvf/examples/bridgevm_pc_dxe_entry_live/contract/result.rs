use super::{pcie, runtime_services, system_table, variable_services, HOB_OFFSET};
use bridgevm_hvf::machine::bridgevm_pc as board;
use std::fmt;

#[derive(Debug, Eq, PartialEq)]
pub struct DxeResult {
    pub(super) system_table: u64,
    pub(super) published: system_table::PublishedTables,
    pub(super) runtime: runtime_services::RuntimeProof,
    pub(super) variable: variable_services::VariableProof,
    pub(super) pcie: pcie::PcieProof,
}

impl fmt::Display for DxeResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "sec_result=1 hob_count=8 hob_list_gpa={:#x} hob_list_size=320 dxe_result={} system_table={:#x} runtime_services={:#x} runtime_protocol={:#x} runtime_crc32={:#x} variable_state={} variable_attributes={:#x} get_variable={:#x} set_variable={:#x} query_variable_info={:#x} variable_max_storage={} variable_remaining_storage={} variable_max_size={} configuration_entries={} acpi={:#x} smbios={:#x} {}",
            board::RAM_BASE + HOB_OFFSET as u64,
            self.variable.state.stage(),
            self.system_table,
            self.runtime.services,
            self.runtime.protocol,
            self.runtime.crc32,
            self.variable.state.raw(),
            self.variable.attributes,
            self.variable.get_variable,
            self.variable.set_variable,
            self.variable.query_variable_info,
            self.variable.maximum_storage,
            self.variable.remaining_storage,
            self.variable.maximum_variable_size,
            self.published.entry_count,
            self.published.acpi,
            self.published.smbios,
            self.pcie
        )
    }
}
