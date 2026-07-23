//! `WindowsArmUefiFirmwareRunLoopProbe::render_text`.
//!
//! NOTE: render_text is a single ~1000-line formatter. Splitting it further
//! requires decomposing the function itself, which is a behaviour-sensitive
//! change rather than a relocation, so it is deliberately left intact here.

use super::*;
use crate::*;

impl WindowsArmUefiFirmwareRunLoopProbe {
    pub(crate) fn low_vector_post_repair_first_exit_telemetry(
        &self,
    ) -> LowVectorPostRepairExitTelemetry {
        LowVectorPostRepairExitTelemetry {
            observed: self.low_vector_post_repair_first_exit_observed,
            index: self.low_vector_post_repair_first_exit_index,
            reason: self.low_vector_post_repair_first_exit_reason,
            diagnosis: self.low_vector_post_repair_first_exit_diagnosis,
            pc: self.low_vector_post_repair_first_exit_pc,
            interaction_kind: self.low_vector_post_repair_first_interaction_kind,
            access: LowVectorPostRepairAccessTelemetry {
                kind: self.low_vector_post_repair_first_exit_access_kind,
                direction: self.low_vector_post_repair_first_exit_access_direction,
                address: self.low_vector_post_repair_first_exit_access_address,
                sysreg: self.low_vector_post_repair_first_exit_access_sysreg,
                syndrome: self.low_vector_post_repair_first_exit_access_syndrome,
            },
        }
    }

    pub(crate) fn low_vector_post_repair_first_device_interaction_telemetry(
        &self,
    ) -> LowVectorPostRepairExitTelemetry {
        LowVectorPostRepairExitTelemetry {
            observed: self.low_vector_post_repair_first_device_interaction_observed,
            index: self.low_vector_post_repair_first_device_interaction_index,
            reason: self.low_vector_post_repair_first_device_interaction_reason,
            diagnosis: self.low_vector_post_repair_first_device_interaction_diagnosis,
            pc: self.low_vector_post_repair_first_device_interaction_pc,
            interaction_kind: self.low_vector_post_repair_first_device_interaction_kind,
            access: LowVectorPostRepairAccessTelemetry {
                kind: self.low_vector_post_repair_first_device_interaction_access_kind,
                direction: self.low_vector_post_repair_first_device_interaction_access_direction,
                address: self.low_vector_post_repair_first_device_interaction_access_address,
                sysreg: self.low_vector_post_repair_first_device_interaction_access_sysreg,
                syndrome: self.low_vector_post_repair_first_device_interaction_access_syndrome,
            },
        }
    }

    pub(crate) fn low_vector_post_repair_first_unhandled_access_telemetry(
        &self,
    ) -> LowVectorPostRepairUnhandledAccessTelemetry {
        LowVectorPostRepairUnhandledAccessTelemetry {
            observed: self.low_vector_post_repair_first_unhandled_access_observed,
            index: self.low_vector_post_repair_first_unhandled_access_index,
            reason: self.low_vector_post_repair_first_unhandled_access_reason,
            diagnosis: self.low_vector_post_repair_first_unhandled_access_diagnosis,
            pc: self.low_vector_post_repair_first_unhandled_access_pc,
            syndrome: self.low_vector_post_repair_first_unhandled_access_syndrome,
            kind: self.low_vector_post_repair_first_unhandled_access_kind,
            access: self.low_vector_post_repair_first_unhandled_access_direction,
            register: self.low_vector_post_repair_first_unhandled_access_register,
            value: self.low_vector_post_repair_first_unhandled_access_value,
            handler_result: self.low_vector_post_repair_first_unhandled_access_handler_result,
            mmio_ipa: self.low_vector_post_repair_first_unhandled_access_mmio_ipa,
            mmio_width: self.low_vector_post_repair_first_unhandled_access_mmio_width,
            mmio_device_kind: self.low_vector_post_repair_first_unhandled_access_mmio_device_kind,
            sysreg: self.low_vector_post_repair_first_unhandled_access_sysreg,
            sysreg_name: self.low_vector_post_repair_first_unhandled_access_sysreg_name,
            sysreg_op0: self.low_vector_post_repair_first_unhandled_access_sysreg_op0,
            sysreg_op1: self.low_vector_post_repair_first_unhandled_access_sysreg_op1,
            sysreg_crn: self.low_vector_post_repair_first_unhandled_access_sysreg_crn,
            sysreg_crm: self.low_vector_post_repair_first_unhandled_access_sysreg_crm,
            sysreg_op2: self.low_vector_post_repair_first_unhandled_access_sysreg_op2,
        }
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
        self.render_setup_state(&mut output);
        self.render_status_fields(&mut output);
        self.render_exits(&mut output);
        self.render_blockers(&mut output);
        output
    }
}
