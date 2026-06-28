use std::collections::BTreeMap;

use bridgevm_hvf::nvme::NvmeCommandTrace;
use bridgevm_hvf::platform_virt::VirtPlatform;

use crate::nvme_storage_effect::{
    nvme_pcie_liveness_attribution_line, nvme_storage_effect_summary,
    nvme_storage_effect_summary_line, NvmePcieLivenessSnapshot,
};

pub(super) fn print_nvme_command_trace(platform: &VirtPlatform) {
    let limit = usize::try_from(super::env_u64("BRIDGEVM_RECENT_NVME_COMMANDS", 32)).unwrap_or(32);
    let trace = platform.nvme_command_trace();
    let liveness = NvmePcieLivenessSnapshot::from(platform.nvme_pcie_liveness());
    println!(
        "{}",
        nvme_pcie_liveness_attribution_line(liveness, nvme_storage_effect_summary(&trace))
    );
    if limit == 0 {
        return;
    }
    if trace.is_empty() {
        return;
    }
    let start = trace.len().saturating_sub(limit);
    println!(
        "recent NVMe commands (last {} of {}):",
        trace.len() - start,
        trace.len()
    );
    for event in &trace[start..] {
        let queue_kind = if event.sqid == 0 { "admin" } else { "io" };
        let interrupt = event
            .completion
            .map(|c| format!("interrupt=cq{}:vector{}", c.cqid, c.vector))
            .unwrap_or_else(|| "interrupt=<none>".to_string());
        let detail = nvme_command_detail(event);
        println!(
            "  {queue_kind} sqid={} cqid={} head={} tail={} entry={:#x} cid={} op={:#04x}({}) nsid={} {} prp1={:#x} prp2={:#x} cdw10={:#010x} cdw11={:#010x} cdw12={:#010x} status={:#06x}({}) posted={} {}",
            event.sqid,
            event.cqid,
            event.sq_head,
            event.sq_tail,
            event.sq_entry_gpa,
            event.command_id,
            event.opcode,
            nvme_opcode_name(event),
            event.nsid,
            detail,
            event.prp1,
            event.prp2,
            event.cdw10,
            event.cdw11,
            event.cdw12,
            event.status,
            nvme_status_name(event.status),
            event.completion_posted,
            interrupt
        );
    }
    println!("{}", nvme_storage_effect_summary_line(&trace[start..]));

    let mut summaries: BTreeMap<String, (usize, u16, u16)> = BTreeMap::new();
    let mut pending = 0usize;
    let mut expected_async_events = 0usize;
    for event in &trace[start..] {
        if !event.completion_posted {
            pending += 1;
            if is_pending_async_event(event) {
                expected_async_events += 1;
            }
        }
        let key = nvme_trace_signature(event);
        summaries
            .entry(key)
            .and_modify(|summary| {
                summary.0 += 1;
                summary.2 = event.command_id;
            })
            .or_insert((1, event.command_id, event.command_id));
    }
    let mut repeated: Vec<_> = summaries
        .into_iter()
        .filter(|(_, (count, _, _))| *count > 1)
        .collect();
    repeated.sort_by(
        |(left_signature, (left_count, _, _)), (right_signature, (right_count, _, _))| {
            right_count
                .cmp(left_count)
                .then_with(|| left_signature.cmp(right_signature))
        },
    );
    if pending != 0 || !repeated.is_empty() {
        println!(
            "recent NVMe summary: pending_without_completion={} expected_async_events={} other_pending={} repeated_signatures={}",
            pending,
            expected_async_events,
            pending.saturating_sub(expected_async_events),
            repeated.len()
        );
        for (signature, (count, first_cid, last_cid)) in repeated.into_iter().take(8) {
            println!("  x{count} cid={first_cid}..{last_cid} {signature}");
        }
    }
}

fn nvme_opcode_name(trace: &NvmeCommandTrace) -> &'static str {
    if trace.sqid == 0 {
        match trace.opcode {
            0x00 => "delete-io-sq",
            0x01 => "create-io-sq",
            0x02 => "get-log-page",
            0x04 => "delete-io-cq",
            0x05 => "create-io-cq",
            0x06 => "identify",
            0x09 => "set-features",
            0x0a => "get-features",
            0x0c => "async-event-request",
            0x81 => "security-send",
            0x82 => "security-receive",
            _ => "admin-unknown",
        }
    } else {
        match trace.opcode {
            0x00 => "flush",
            0x01 => "write",
            0x02 => "read",
            _ => "io-unknown",
        }
    }
}

fn nvme_status_name(status: u16) -> &'static str {
    match status {
        0x0000 => "success",
        0x0001 => "invalid-opcode",
        0x0002 => "invalid-field",
        0x4002 => "invalid-field-dnr",
        _ => "unknown",
    }
}

fn nvme_command_detail(trace: &NvmeCommandTrace) -> String {
    match (trace.sqid, trace.opcode) {
        (0, 0x00 | 0x04) => {
            let qid = trace.cdw10 & 0xffff;
            format!("qid={qid}")
        }
        (0, 0x01) => {
            let sqid = trace.cdw10 & 0xffff;
            let qsize = (trace.cdw10 >> 16) + 1;
            let cqid = trace.cdw11 >> 16;
            let qflags = trace.cdw11 & 0xffff;
            format!("sqid={sqid} cqid={cqid} qsize={qsize} qflags={qflags:#06x}")
        }
        (0, 0x02) => {
            let lid = trace.cdw10 & 0xff;
            let numd = (((trace.cdw11 & 0xffff) << 16) | ((trace.cdw10 >> 16) & 0xffff)) + 1;
            format!("lid={lid:#04x} numd={numd}")
        }
        (0, 0x05) => {
            let cqid = trace.cdw10 & 0xffff;
            let qsize = (trace.cdw10 >> 16) + 1;
            let vector = trace.cdw11 >> 16;
            let qflags = trace.cdw11 & 0xffff;
            format!("cqid={cqid} vector={vector} qsize={qsize} qflags={qflags:#06x}")
        }
        (0, 0x06) => {
            let cns = trace.cdw10 & 0xff;
            let csi = (trace.cdw11 >> 24) & 0xff;
            format!("cns={cns:#04x} csi={csi:#04x}")
        }
        (0, 0x09 | 0x0a) => {
            let fid = trace.cdw10 & 0xff;
            let sel = (trace.cdw10 >> 8) & 0x7;
            format!("fid={fid:#04x} sel={sel:#x} value={:#010x}", trace.cdw11)
        }
        (0, 0x81 | 0x82) => {
            let protocol = (trace.cdw10 >> 24) & 0xff;
            let specific = (trace.cdw10 >> 8) & 0xffff;
            let len = trace.cdw11;
            format!("sec_proto={protocol:#04x} specific={specific:#06x} len={len}")
        }
        (sqid, 0x01 | 0x02) if sqid != 0 => {
            let lba = (u64::from(trace.cdw11) << 32) | u64::from(trace.cdw10);
            let blocks = (trace.cdw12 & 0xffff) + 1;
            let flags = trace.cdw12 & !0xffff;
            format!(
                "lba={lba} blocks={blocks} bytes={} flags={flags:#010x}",
                u64::from(blocks) * 512
            )
        }
        (sqid, 0x00) if sqid != 0 => "flush".to_string(),
        _ => format!(
            "cdw10={:#010x} cdw11={:#010x} cdw12={:#010x}",
            trace.cdw10, trace.cdw11, trace.cdw12
        ),
    }
}

fn nvme_trace_signature(trace: &NvmeCommandTrace) -> String {
    format!(
        "{} op={:#04x}({}) nsid={} {} status={:#06x}({}) posted={}",
        if trace.sqid == 0 { "admin" } else { "io" },
        trace.opcode,
        nvme_opcode_name(trace),
        trace.nsid,
        nvme_command_detail(trace),
        trace.status,
        nvme_status_name(trace.status),
        trace.completion_posted
    )
}

fn is_pending_async_event(trace: &NvmeCommandTrace) -> bool {
    trace.sqid == 0 && trace.opcode == 0x0c && !trace.completion_posted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(sqid: u16, opcode: u8) -> NvmeCommandTrace {
        NvmeCommandTrace {
            sqid,
            cqid: 0,
            sq_head: 0,
            sq_tail: 0,
            sq_entry_gpa: 0,
            opcode,
            command_id: 0,
            nsid: 0,
            prp1: 0,
            prp2: 0,
            cdw10: 0,
            cdw11: 0,
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
            status: 0,
            completion_posted: true,
            completion: None,
        }
    }

    #[test]
    fn create_io_sq_detail_decodes_cqid_from_cdw11_high_half() {
        let t = NvmeCommandTrace {
            cdw10: 0x003f_0001,
            cdw11: 0x0001_0005,
            ..trace(0, 0x01)
        };
        assert_eq!(
            nvme_command_detail(&t),
            "sqid=1 cqid=1 qsize=64 qflags=0x0005"
        );
    }

    #[test]
    fn async_event_request_is_the_expected_pending_admin_command() {
        let mut t = trace(0, 0x0c);
        t.completion_posted = false;
        assert!(is_pending_async_event(&t));
    }
}
