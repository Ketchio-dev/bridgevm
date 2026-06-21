#[derive(Clone, Copy)]
pub(crate) struct EventRingTrace {
    pub(crate) segment_base: u64,
    pub(crate) segment_trbs: u32,
    pub(crate) enqueue: u32,
    pub(crate) cycle: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct EventPostTrace {
    pub(crate) ring: EventRingTrace,
    pub(crate) parameter: u64,
    pub(crate) status: u32,
    pub(crate) control: u32,
    pub(crate) event_gpa: u64,
}

pub(crate) fn bringup_enabled() -> bool {
    matches!(
        std::env::var("BRIDGEVM_TRACE_XHCI_BRINGUP").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

pub(crate) fn event_post_reject(reason: &str) {
    if bringup_enabled() {
        println!("XHCI event post result posted=false reason={reason}");
    }
}

pub(crate) fn event_post_reject_with_gpa(reason: &str, gpa: u64) {
    if bringup_enabled() {
        println!("XHCI event post result posted=false reason={reason} gpa={gpa:#x}");
    }
}

pub(crate) fn event_post_reject_with_ring(reason: &str, trace: EventRingTrace) {
    if bringup_enabled() {
        println!(
            "XHCI event post result posted=false reason={reason} segment_base={segment_base:#x} segment_trbs={segment_trbs} enqueue={enqueue} cycle={cycle}",
            segment_base = trace.segment_base,
            segment_trbs = trace.segment_trbs,
            enqueue = trace.enqueue,
            cycle = u32::from(trace.cycle)
        );
    }
}

pub(crate) fn event_post_reject_with_event(reason: &str, trace: EventPostTrace) {
    if bringup_enabled() {
        println!(
            "XHCI event post result posted=false reason={reason} parameter={parameter:#x} status={status:#010x} control={control:#010x} event_gpa={event_gpa:#x} segment_base={segment_base:#x} segment_trbs={segment_trbs} enqueue={enqueue} cycle={cycle}",
            parameter = trace.parameter,
            status = trace.status,
            control = trace.control,
            event_gpa = trace.event_gpa,
            segment_base = trace.ring.segment_base,
            segment_trbs = trace.ring.segment_trbs,
            enqueue = trace.ring.enqueue,
            cycle = u32::from(trace.ring.cycle)
        );
    }
}

pub(crate) fn event_post_success(trace: EventPostTrace) {
    if bringup_enabled() {
        println!(
            "XHCI event post result posted=true parameter={parameter:#x} status={status:#010x} control={control:#010x} event_gpa={event_gpa:#x} segment_base={segment_base:#x} segment_trbs={segment_trbs} enqueue={enqueue} cycle={cycle}",
            parameter = trace.parameter,
            status = trace.status,
            control = trace.control,
            event_gpa = trace.event_gpa,
            segment_base = trace.ring.segment_base,
            segment_trbs = trace.ring.segment_trbs,
            enqueue = trace.ring.enqueue,
            cycle = u32::from(trace.ring.cycle)
        );
    }
}

pub(crate) fn ep0_handler_entered(transfer_ring: u64) {
    if bringup_enabled() {
        println!("XHCI EP0 handler entered transfer_ring={transfer_ring:#x}");
    }
}

pub(crate) fn ep0_trb(label: &str, gpa: u64, parameter: u64, status: u32, control: u32, ty: u32) {
    if bringup_enabled() {
        println!(
            "XHCI EP0 {label}_trb gpa={gpa:#x} parameter={parameter:#x} status={status:#010x} control={control:#010x} type={ty}"
        );
    }
}

pub(crate) fn ep0_setup_packet(
    bm_request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
) {
    if bringup_enabled() {
        println!(
            "XHCI EP0 setup_packet bm_request_type={bm_request_type:#04x} request={request:#04x} value={value:#06x} index={index} length={length}"
        );
    }
}

pub(crate) fn ep0_descriptor_write_success(target_gpa: u64, len: usize) {
    if bringup_enabled() {
        println!("XHCI EP0 descriptor_write success target_gpa={target_gpa:#x} len={len}");
    }
}

pub(crate) fn ep0_post_event_request(parameter: u64, status: u32, control: u32) {
    if bringup_enabled() {
        println!(
            "XHCI EP0 post_event request parameter={parameter:#x} status={status:#010x} control={control:#010x}"
        );
    }
}

pub(crate) fn ep0_post_event_result(posted: bool) {
    if bringup_enabled() {
        println!("XHCI EP0 post_event result posted={posted}");
    }
}

pub(crate) fn ep0_reject(reason: &str) {
    if bringup_enabled() {
        println!("XHCI EP0 outcome posted=false reason={reason}");
    }
}

pub(crate) fn ep0_reject_with_gpa(reason: &str, gpa: u64) {
    if bringup_enabled() {
        println!("XHCI EP0 outcome posted=false reason={reason} gpa={gpa:#x}");
    }
}

pub(crate) fn ep0_reject_with_value(reason: &str, value: u32) {
    if bringup_enabled() {
        println!("XHCI EP0 outcome posted=false reason={reason} value={value:#x}");
    }
}
