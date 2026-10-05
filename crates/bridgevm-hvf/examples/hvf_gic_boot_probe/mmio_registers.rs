//! Native register transfers for a decoded MMIO access.

use crate::{hv_vcpu_get_reg, hv_vcpu_set_reg, HvVcpuT, HV_REG_X0};
use bridgevm_hvf::mmio_data_abort::DataAbort;

pub(crate) unsafe fn write_mmio_read(vcpu: HvVcpuT, access: DataAbort, value: u64) {
    if let Some(value) = access.read_result(value) {
        hv_vcpu_set_reg(vcpu, HV_REG_X0 + access.register, value);
    }
}

pub(crate) unsafe fn mmio_store_value(vcpu: HvVcpuT, access: DataAbort) -> u64 {
    let mut value = 0;
    if access.register != 31 {
        hv_vcpu_get_reg(vcpu, HV_REG_X0 + access.register, &mut value);
    }
    value
}
