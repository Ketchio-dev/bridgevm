use super::*;
use std::cell::Cell;

const FAILED: HvReturn = -345;
fn context(location: DrainLocation) -> DrainContext {
    DrainContext {
        location,
        exit: 7,
        pc: 0x1234,
    }
}
fn prepared(
    stats: &mut RunLoopDrainStats,
    location: DrainLocation,
) -> Result<PendingDrainDelivery, SpiDeliveryError> {
    let mut pending = vec![(43, true), (43, false)];
    let delivered = deliver_spi_levels(&mut pending, |_, _| FAILED);
    stats.pending_spi_delivery(context(location), delivered)
}
fn primary_boundary(location: DrainLocation) {
    let mut stats = RunLoopDrainStats::new(false);
    let pending = prepared(&mut stats, location);
    let completed = Cell::new(0);
    let next_guest_stage = Cell::new(0);
    let (mut fatal, mut pc, mut reason) = (false, 0x9999, String::new());
    match complete_prepared_interrupt_delivery(pending, |pending| {
        completed.set(completed.get() + 1);
        stats.complete_pending_delivery(
            pending,
            DrainTrace {
                msix: false,
                spi: false,
            },
        )
    }) {
        Ok(()) => next_guest_stage.set(next_guest_stage.get() + 1),
        Err(error) => reason = error.stop_primary(&mut fatal, &mut pc),
    }
    assert!(
        fatal,
        "failed SPI allowed {} continuation; completed={} next={}",
        location.as_str(),
        completed.get(),
        next_guest_stage.get()
    );
    assert_eq!((completed.get(), next_guest_stage.get()), (0, 0));
    assert_eq!(pc, 0x1234);
    assert!(reason.contains("intid=43 level=true status=0xfffffea7"));
    assert!(reason.contains(location.as_str()));
    assert_eq!(
        (stats.spi.drained, stats.spi.success, stats.spi.failure),
        (1, 0, 1)
    );
    assert_eq!(stats.msix.drained, 0);
}
fn secondary_boundary(location: DrainLocation) {
    let mut stats = RunLoopDrainStats::new(false);
    let pending = prepared(&mut stats, location);
    let (completed, next_guest_stage, fatal, wakes) =
        (Cell::new(0), Cell::new(0), Cell::new(false), Cell::new(0));
    let result = complete_prepared_interrupt_delivery(pending, |pending| {
        completed.set(completed.get() + 1);
        stats.complete_pending_delivery(
            pending,
            DrainTrace {
                msix: false,
                spi: false,
            },
        )
    });
    match result {
        Ok(()) => next_guest_stage.set(next_guest_stage.get() + 1),
        Err(error) => error.stop_secondary(|| {
            fatal.set(true);
            wakes.set(wakes.get() + 1);
        }),
    }
    assert!(
        fatal.get(),
        "failed secondary SPI allowed {} continuation; completed={} next={}",
        location.as_str(),
        completed.get(),
        next_guest_stage.get()
    );
    assert_eq!(
        (completed.get(), next_guest_stage.get(), wakes.get()),
        (0, 0, 1)
    );
    assert_eq!((stats.spi.drained, stats.spi.failure), (1, 1));
    assert_eq!(stats.msix.drained, 0);
}

#[test]
fn cpu0_pre_run_spi_failure_blocks_completion_and_guest_entry() {
    primary_boundary(DrainLocation::PreRun);
}
#[test]
fn cpu0_post_mmio_spi_failure_blocks_completion_and_register_pc_stage() {
    primary_boundary(DrainLocation::DataAbort);
}
#[test]
fn secondary_pre_run_spi_failure_blocks_completion_and_guest_entry() {
    secondary_boundary(DrainLocation::PreRun);
}
#[test]
fn secondary_post_mmio_spi_failure_blocks_completion_and_register_pc_stage() {
    secondary_boundary(DrainLocation::DataAbort);
}

#[test]
fn first_spi_error_stops_later_requests_without_replaying_accepted_request() {
    let mut pending = vec![(43, true), (44, true), (43, false)];
    let mut calls = Vec::new();
    let result = deliver_spi_levels(&mut pending, |intid, level| {
        calls.push((intid, level));
        if intid == 44 {
            FAILED
        } else {
            0
        }
    });
    assert!(
        result.is_err(),
        "nonzero status was discarded; actual calls={calls:?}"
    );
    let error = result.err().unwrap();
    assert_eq!((error.intid, error.level, error.status), (44, true, FAILED));
    assert_eq!(
        (
            error.counts.drained,
            error.counts.success,
            error.counts.failure
        ),
        (2, 1, 1)
    );
    assert_eq!(calls, vec![(43, true), (44, true)]);
    assert!(pending.is_empty());
    assert!(deliver_spi_levels(&mut pending, |intid, level| {
        calls.push((intid, level));
        0
    })
    .is_ok());
    assert_eq!(calls.len(), 2);
}
#[test]
fn failing_spi_deassertion_preserves_exact_positive_provider_status() {
    let mut pending = vec![(43, false), (43, true)];
    let result = deliver_spi_levels(&mut pending, |_, _| 42);
    assert!(result.is_err(), "failed deassertion was silently accepted");
    let error = result.err().unwrap();
    assert_eq!((error.intid, error.level, error.status), (43, false, 42));
    assert_eq!((error.counts.drained, error.counts.failure), (1, 1));
    assert!(pending.is_empty());
}
#[test]
fn successful_spi_batch_completes_once_and_counts_each_request_once() {
    let mut stats = RunLoopDrainStats::new(false);
    let mut pending = vec![(43, true), (43, false)];
    let delivered = deliver_spi_levels(&mut pending, |_, _| 0);
    let ready = stats.pending_spi_delivery(context(DrainLocation::PreRun), delivered);
    let completed = Cell::new(0);
    assert!(complete_prepared_interrupt_delivery(ready, |ready| {
        completed.set(completed.get() + 1);
        stats.complete_pending_delivery(
            ready,
            DrainTrace {
                msix: false,
                spi: false,
            },
        )
    })
    .is_ok());
    assert_eq!(completed.get(), 1);
    assert_eq!(
        (stats.spi.drained, stats.spi.success, stats.spi.failure),
        (2, 2, 0)
    );
    assert!(pending.is_empty());
}
#[test]
fn empty_spi_batch_never_invokes_provider() {
    let result = deliver_spi_levels(&mut Vec::new(), |_, _| {
        panic!("empty batch must not call provider")
    });
    assert!(result.is_ok());
    let counts = result.ok().unwrap();
    assert_eq!((counts.drained, counts.success, counts.failure), (0, 0, 0));
}

#[path = "spi_finish_tests.rs"]
mod finish;
