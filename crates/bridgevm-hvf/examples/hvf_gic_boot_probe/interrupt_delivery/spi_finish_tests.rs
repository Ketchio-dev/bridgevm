use super::*;

#[test]
fn production_finish_adapter_propagates_all_caller_contexts_and_counts_failure_once() {
    for location in [DrainLocation::PreRun, DrainLocation::DataAbort] {
        for secondary in [false, true] {
            let mut stats = RunLoopDrainStats::new(false);
            let pending = prepared(&mut stats, location);
            let result = stats.finish_prepared_interrupt_delivery(
                pending,
                DrainTrace {
                    msix: false,
                    spi: false,
                },
            );
            assert!(
                result.is_err(),
                "finish adapter discarded SPI failure secondary={secondary} location={}",
                location.as_str()
            );
            let error = result.err().unwrap();
            assert!(
                matches!(&error, InterruptDeliveryError::Spi(error) if error.failure.status == FAILED)
            );
            if secondary {
                error.stop_secondary(|| {});
            } else {
                let (mut fatal, mut pc) = (false, 0);
                error.stop_primary(&mut fatal, &mut pc);
                assert!(fatal);
                assert_eq!(pc, 0x1234);
            }
            assert_eq!(
                (stats.spi.drained, stats.spi.failure, stats.msix.drained),
                (1, 1, 0)
            );
        }
    }
}
