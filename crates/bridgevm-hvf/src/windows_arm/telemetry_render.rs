//! Split out of windows_arm.rs by responsibility.

use super::*;
use crate::*;

pub(crate) fn append_low_vector_post_repair_exit_telemetry(
    output: &mut String,
    label: &str,
    telemetry: &LowVectorPostRepairExitTelemetry,
    kind_label: &str,
    context_exit: Option<&WindowsArmUefiFirmwareRunLoopExit>,
) {
    output.push_str(&format!("{label} observed: {}\n", telemetry.observed));
    output.push_str(&format!(
        "{label}: {}\n",
        render_optional_intid(telemetry.index)
    ));
    output.push_str(&format!(
        "{label} reason name: {}\n",
        render_optional_exit_reason_name(telemetry.reason)
    ));
    output.push_str(&format!(
        "{label} classification: {}\n",
        telemetry.diagnosis
    ));
    output.push_str(&format!(
        "{label} PC: {}\n",
        render_optional_u64(telemetry.pc)
    ));
    output.push_str(&format!(
        "{label} instruction: {}\n",
        render_optional_instruction_word(
            context_exit.and_then(|exit| exit.instruction_word_after_exit)
        )
    ));
    output.push_str(&format!(
        "{label} instruction hint: {}\n",
        context_exit
            .map(|exit| exit.instruction_hint_after_exit)
            .unwrap_or("not observed")
    ));
    output.push_str(&format!(
        "{label} VBAR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.vbar_el1_after_exit))
    ));
    output.push_str(&format!(
        "{label} ELR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.elr_el1_after_exit))
    ));
    output.push_str(&format!(
        "{label} ESR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.esr_el1_after_exit))
    ));
    output.push_str(&format!(
        "{label} FAR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.far_el1_after_exit))
    ));
    output.push_str(&format!(
        "{label} SPSR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.spsr_el1_after_exit))
    ));
    output.push_str(&format!("{label} access kind: {}\n", telemetry.access.kind));
    output.push_str(&format!(
        "{label} access direction: {}\n",
        telemetry.access.direction
    ));
    output.push_str(&format!(
        "{label} access address: {}\n",
        render_optional_u64(telemetry.access.address)
    ));
    output.push_str(&format!(
        "{label} access sysreg: {}\n",
        render_optional_u16_hex(telemetry.access.sysreg)
    ));
    output.push_str(&format!(
        "{label} access syndrome: {}\n",
        render_optional_u64(telemetry.access.syndrome)
    ));
    output.push_str(&format!("{kind_label}: {}\n", telemetry.interaction_kind));
}

pub(crate) fn append_low_vector_post_repair_unhandled_access_telemetry(
    output: &mut String,
    label: &str,
    telemetry: &LowVectorPostRepairUnhandledAccessTelemetry,
) {
    output.push_str(&format!("{label} observed: {}\n", telemetry.observed));
    output.push_str(&format!(
        "{label}: {}\n",
        render_optional_intid(telemetry.index)
    ));
    output.push_str(&format!(
        "{label} reason name: {}\n",
        render_optional_exit_reason_name(telemetry.reason)
    ));
    output.push_str(&format!(
        "{label} classification: {}\n",
        telemetry.diagnosis
    ));
    output.push_str(&format!(
        "{label} PC: {}\n",
        render_optional_u64(telemetry.pc)
    ));
    output.push_str(&format!(
        "{label} syndrome: {}\n",
        render_optional_u64(telemetry.syndrome)
    ));
    output.push_str(&format!("{label} kind: {}\n", telemetry.kind));
    output.push_str(&format!("{label} direction: {}\n", telemetry.access));
    output.push_str(&format!(
        "{label} register: {}\n",
        render_optional_u8(telemetry.register)
    ));
    output.push_str(&format!(
        "{label} value: {}\n",
        render_optional_u64(telemetry.value)
    ));
    output.push_str(&format!(
        "{label} handler result: {}\n",
        telemetry.handler_result
    ));
    output.push_str(&format!(
        "{label} MMIO IPA: {}\n",
        render_optional_u64(telemetry.mmio_ipa)
    ));
    output.push_str(&format!(
        "{label} MMIO width: {}\n",
        render_optional_u8(telemetry.mmio_width)
    ));
    output.push_str(&format!(
        "{label} MMIO device kind: {}\n",
        telemetry.mmio_device_kind
    ));
    output.push_str(&format!(
        "{label} sysreg: {}\n",
        render_optional_u16_hex(telemetry.sysreg)
    ));
    output.push_str(&format!("{label} sysreg name: {}\n", telemetry.sysreg_name));
    output.push_str(&format!(
        "{label} sysreg op0: {}\n",
        render_optional_u8(telemetry.sysreg_op0)
    ));
    output.push_str(&format!(
        "{label} sysreg op1: {}\n",
        render_optional_u8(telemetry.sysreg_op1)
    ));
    output.push_str(&format!(
        "{label} sysreg crn: {}\n",
        render_optional_u8(telemetry.sysreg_crn)
    ));
    output.push_str(&format!(
        "{label} sysreg crm: {}\n",
        render_optional_u8(telemetry.sysreg_crm)
    ));
    output.push_str(&format!(
        "{label} sysreg op2: {}\n",
        render_optional_u8(telemetry.sysreg_op2)
    ));
}

pub(crate) fn low_vector_post_repair_context_exit(
    exits: &[WindowsArmUefiFirmwareRunLoopExit],
    index: Option<u32>,
) -> Option<&WindowsArmUefiFirmwareRunLoopExit> {
    let index = index?;
    exits.iter().find(|exit| exit.index == index)
}

pub(crate) fn render_optional_u32(value: Option<u32>) -> String {
    value.map_or_else(|| "unknown".to_string(), |value| value.to_string())
}

pub(crate) fn render_optional_u16_hex(value: Option<u16>) -> String {
    value.map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
}

pub(crate) fn render_optional_intid(value: Option<u32>) -> String {
    value.map_or_else(|| "not observed".to_string(), |value| value.to_string())
}

pub(crate) fn render_optional_gic_intid(value: Option<u32>) -> String {
    value.map_or_else(
        || "not observed".to_string(),
        |value| match value {
            GICV3_SPURIOUS_INTERRUPT_ID => "spurious".to_string(),
            value => value.to_string(),
        },
    )
}

pub(crate) fn render_optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
}

pub(crate) fn render_optional_u8(value: Option<u8>) -> String {
    value.map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
}

pub(crate) fn render_optional_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "not observed",
    }
}

pub(crate) fn render_optional_instruction_word(value: Option<u32>) -> String {
    value.map_or_else(
        || "not observed".to_string(),
        |value| format!("{value:#010x}"),
    )
}

pub(crate) fn render_hex_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "not observed".to_string();
    }
    let mut output = String::from("0x");
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

pub(crate) fn render_optional_status(value: Option<i32>) -> String {
    value.map_or_else(
        || "not attempted".to_string(),
        |status| format!("{status:#x}"),
    )
}

pub(crate) fn render_optional_status_name(value: Option<i32>) -> &'static str {
    value.map_or("not attempted", hv_return_name)
}

pub(crate) fn render_optional_exit_reason(value: Option<u32>) -> String {
    value.map_or_else(
        || "not observed".to_string(),
        |reason| format!("{reason:#x}"),
    )
}

pub(crate) fn render_optional_exit_reason_name(value: Option<u32>) -> &'static str {
    value.map_or("not observed", hv_exit_reason_name)
}

pub(crate) fn render_optional_exception_class_name(value: Option<u64>) -> &'static str {
    value.map_or("not observed", arm_exception_class_name)
}

pub(crate) fn render_optional_esr_exception_class_name(value: Option<u64>) -> &'static str {
    value.map_or("not observed", |esr| {
        arm_exception_class_name(arm_exception_class(esr))
    })
}

pub(crate) fn render_optional_sctlr_mmu_enabled(value: Option<u64>) -> &'static str {
    match value {
        Some(sctlr) if sctlr & 1 == 1 => "true",
        Some(_) => "false",
        None => "not observed",
    }
}

pub(crate) fn windows_arm_initial_sp_el1_ipa(guest_ram_bytes: u64) -> u64 {
    WINDOWS_ARM_GUEST_RAM_IPA
        .saturating_add(guest_ram_bytes)
        .saturating_sub(16)
        & !0xf
}
