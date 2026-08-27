use super::super::pointer_input::XhciPointerInputTrigger;
use super::super::test_support::{
    configure_dci3_and_dci5_interrupt_in_over_bar0, emit_uart, new_platform_and_ram,
    program_xhci_bar0, write_dci5_normal_trb, DCI5_POINTER_BUFFER, DCI5_RING, TRB_SIZE,
};
use std::time::{Duration, Instant};

#[test]
fn pending_pointer_wake_uses_the_earliest_future_deadline() {
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_and_dci5_interrupt_in_over_bar0(&mut platform, &mut mem);
    for index in 0..2 {
        write_dci5_normal_trb(
            &mut mem,
            DCI5_RING + TRB_SIZE * index,
            DCI5_POINTER_BUFFER + 0x20 * index,
        );
    }
    platform.set_xhci_report_interval(Duration::from_secs(5));
    let fired_at = Instant::now();
    let mut trigger = XhciPointerInputTrigger::from_env_value_with_ramfb_delay_ms(
        "pointer-input",
        "click:center",
        &[1_000],
    )
    .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot");
    assert!(trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
        &mut platform,
        &mut mem,
        fired_at,
        |_, _, _| {},
    ));
    assert_eq!(
        platform.xhci_pointer_report_deadline(),
        Some(fired_at + Duration::from_secs(5))
    );
    assert_eq!(
        trigger.pending_host_wake_deadline_at(&platform, fired_at),
        Some(fired_at + Duration::from_secs(1))
    );
}
