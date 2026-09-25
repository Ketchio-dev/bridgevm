use super::*;

fn captured(value: u64) -> CapturedRegister {
    CapturedRegister { status: 0, value }
}

fn refused(status: i32) -> CapturedRegister {
    CapturedRegister { status, value: 0 }
}

#[test]
fn failed_timer_mask_mpidr_and_pstate_reads_never_become_measured_zeroes() {
    let state = OwnerInterruptState {
        cntv_ctl: refused(0xfae9_4003u32 as i32),
        cntv_cval: refused(0xfae9_4004u32 as i32),
        cntp_ctl: refused(0xfae9_4005u32 as i32),
        cntp_cval: refused(0xfae9_4006u32 as i32),
        vtimer_offset: refused(0xfae9_4007u32 as i32),
        host_vtimer_mask: CapturedMask {
            status: 0xfae9_4008u32 as i32,
            value: false,
        },
        host_counter: 123,
        gic: Default::default(),
    };
    let lines = state.render(2, 7, 0x8000_0002, refused(-1), refused(-2));
    assert_eq!(lines.len(), 5);
    assert!(lines[0].contains("VCPU-IRQ[2]: generation=7 expected_mpidr=0x80000002"));
    assert!(lines[0].contains("measured_mpidr=<read-failed:"));
    assert!(lines[0].contains("pstate_irq_masked=<unavailable:cpsr-read>"));
    assert!(lines[1].contains("CNTV_CTL=<read-failed:"));
    assert!(lines[1].contains("vtimer_offset=<read-failed:"));
    assert!(lines[1].contains("guest_now=<unavailable:vtimer-offset-read>"));
    assert!(lines[1].contains("host_vtimer_masked=<read-failed:"));
    assert!(lines[3].contains("vtimer_verdict=unavailable"));
    assert!(!lines.join("\n").contains("stall=true"));
}

#[test]
fn owner_report_preserves_cpu_identity_and_valid_timer_mask_values() {
    let state = OwnerInterruptState {
        cntv_ctl: captured(0b101),
        cntv_cval: captured(0x80),
        cntp_ctl: captured(0),
        cntp_cval: captured(0x90),
        vtimer_offset: captured(0x20),
        host_vtimer_mask: CapturedMask {
            status: 0,
            value: true,
        },
        host_counter: 0xa0,
        gic: Default::default(),
    };
    let primary = state.render(0, 3, 0x8000_0000, captured(0x8000_0000), captured(0x80));
    let secondary = state.render(2, 3, 0x8000_0002, captured(0x8000_0002), captured(0));
    assert!(primary.iter().all(|line| line.starts_with("VCPU-IRQ[0]")));
    assert!(secondary.iter().all(|line| line.starts_with("VCPU-IRQ[2]")));
    assert!(primary[0].contains("pstate_irq_masked=true"));
    assert!(secondary[0].contains("pstate_irq_masked=false"));
    assert!(secondary[1].contains("CNTV_CTL=0x5 CNTV_CVAL=0x80"));
    assert!(secondary[1].contains("guest_now=0x80 host_vtimer_masked=true"));
}

#[test]
fn default_capture_is_explicitly_unavailable() {
    let state = OwnerInterruptState::default();
    assert_eq!(state.cntv_ctl.value(), None);
    assert_eq!(state.vtimer_offset.value(), None);
    assert_ne!(state.host_vtimer_mask.status, 0);
    assert!(state.render(1, 1, 0x8000_0001, refused(-1), refused(-1))[1]
        .contains("guest_now=<unavailable:vtimer-offset-read>"));
}
