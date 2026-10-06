use super::*;
use std::cell::Cell;

const FAILED: HvReturn = 0xfae9_4001u32 as i32;
fn message(vector: u16) -> MsixMessage {
    MsixMessage {
        vector,
        address: machine::GIC_MSI_FRAME.base + 0x40,
        data: machine::GIC_MSI_INTID_BASE + u32::from(vector),
    }
}
fn context(location: DrainLocation) -> DrainContext {
    DrainContext {
        location,
        exit: 7,
        pc: 0x1234,
    }
}
fn boundary(location: DrainLocation, secondary: bool) {
    let mut stats = RunLoopDrainStats::new(false);
    stats.pending_msix_scratch = vec![message(0), message(1)];
    let pending = stats.pending_spi_delivery(
        context(location),
        Ok(DeliveryCounts {
            drained: 2,
            success: 2,
            failure: 0,
        }),
    );
    let calls = Cell::new(0);
    let result = complete_prepared_interrupt_delivery(pending, |pending| {
        stats.complete_pending_delivery_with(pending, |messages| {
            deliver_msix_messages(messages, |_| {
                calls.set(calls.get() + 1);
                FAILED
            })
        })
    });
    let (mut fatal, mut pc, mut continuation, mut wakes) = (false, 0x9999, 0, 0);
    let mut reason = String::new();
    match result {
        Ok(()) => continuation += 1,
        Err(error) if secondary => error.stop_secondary(|| {
            fatal = true;
            wakes += 1;
        }),
        Err(error) => reason = error.stop_primary(&mut fatal, &mut pc),
    }
    assert!(fatal, "MSI-X error swallowed: location={} secondary={secondary} calls={} continuation={continuation}", location.as_str(), calls.get());
    assert_eq!((calls.get(), continuation), (1, 0));
    if secondary {
        assert_eq!(wakes, 1);
    } else {
        assert_eq!(pc, 0x1234);
        assert!(reason.contains("vector=0 address=0x8080040 data=0x80 status=0xfae94001"));
        assert!(reason.contains(location.as_str()));
    }
    assert_eq!(
        (stats.spi.drained, stats.spi.success, stats.spi.failure),
        (2, 2, 0)
    );
    assert_eq!(
        (stats.msix.drained, stats.msix.success, stats.msix.failure),
        (1, 0, 1)
    );
    assert_eq!(stats.last_drain_pc, Some(0x1234));
    assert_eq!(stats.last_nonzero_location, Some(location.as_str()));
    assert!(stats.pending_msix_scratch.is_empty());
}

#[test]
fn cpu0_pre_run_msix_failure_blocks_guest_entry() {
    boundary(DrainLocation::PreRun, false);
}
#[test]
fn cpu0_post_mmio_msix_failure_blocks_register_and_pc_stage() {
    boundary(DrainLocation::DataAbort, false);
}
#[test]
fn secondary_pre_run_msix_failure_uses_existing_fatal_channel() {
    boundary(DrainLocation::PreRun, true);
}
#[test]
fn secondary_post_mmio_msix_failure_blocks_register_and_pc_stage() {
    boundary(DrainLocation::DataAbort, true);
}

#[test]
fn first_msix_error_stops_unattempted_tail_and_preserves_successful_prefix() {
    let messages = [message(0), message(1), message(2)];
    let mut calls = Vec::new();
    let result = deliver_msix_messages(&messages, |message| {
        calls.push(message);
        if message.vector == 1 {
            FAILED
        } else {
            0
        }
    });
    assert!(
        result.is_err(),
        "provider failure discarded: calls={calls:?}"
    );
    let failure = result.err().unwrap();
    assert_eq!(calls, messages[..2]);
    assert_eq!(failure.message, messages[1]);
    assert_eq!(failure.status, FAILED);
    assert_eq!(
        (
            failure.counts.drained,
            failure.counts.success,
            failure.counts.failure
        ),
        (2, 1, 1)
    );
}

#[test]
fn successful_and_empty_msix_completions_account_once_without_replay() {
    let mut stats = RunLoopDrainStats::new(false);
    stats.pending_msix_scratch = vec![message(0), message(1)];
    let mut calls = Vec::new();
    for round in 0..2 {
        let pending = stats.pending_spi_delivery(
            context(DrainLocation::PreRun),
            Ok(DeliveryCounts {
                drained: u64::from(round == 0),
                success: u64::from(round == 0),
                failure: 0,
            }),
        );
        let result = complete_prepared_interrupt_delivery(pending, |pending| {
            stats.complete_pending_delivery_with(pending, |messages| {
                deliver_msix_messages(messages, |message| {
                    calls.push(message);
                    0
                })
            })
        });
        assert!(result.is_ok());
        assert!(stats.pending_msix_scratch.is_empty());
    }
    assert_eq!(calls, [message(0), message(1)]);
    assert_eq!(
        (stats.spi.drained, stats.spi.success, stats.spi.failure),
        (1, 1, 0)
    );
    assert_eq!(
        (stats.msix.drained, stats.msix.success, stats.msix.failure),
        (2, 2, 0)
    );
    assert_eq!(stats.last_drain_was_empty(), Some(true));
}

#[test]
fn completion_error_is_not_hidden_by_prepared_delivery_adapter() {
    let pending = Ok(PendingDrainDelivery {
        context: context(DrainLocation::DataAbort),
        spi: DeliveryCounts::default(),
    });
    let result = complete_prepared_interrupt_delivery(pending, |pending| {
        Err(InterruptDeliveryError::Msix {
            context: pending.context,
            failure: MsixDeliveryFailure {
                message: message(0),
                status: FAILED,
                counts: DeliveryCounts {
                    drained: 1,
                    success: 0,
                    failure: 1,
                },
            },
        })
    });
    assert!(result.is_err(), "completion callback error was swallowed");
}
