//! Bounded receipt for the vtimer/cancellation microprobe.
//!
//! The verdict is computed here, away from any Hypervisor.framework call, so
//! the pass/fail rule is unit-testable and cannot drift from the JSON that
//! ships as evidence.

/// Counters accumulated over a probe run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Counters {
    /// Iterations the guest was asked to complete.
    pub(crate) iterations: u64,
    /// Timer wakes observed by the guest IRQ handler.
    pub(crate) timer_wakes: u64,
    /// `EXIT_CANCELED` exits the host observed.
    pub(crate) canceled_exits: u64,
    /// Cancels that arrived with no host request outstanding to claim them.
    pub(crate) surplus_canceled: u64,
    /// Times the host found the vtimer masked with the deadline already past.
    pub(crate) masked_past_deadline: u64,
    /// Recoveries the host applied after a swallowed fire.
    pub(crate) recoveries: u64,
    /// `EXIT_VTIMER` exits, which in-kernel GIC delivery should not produce.
    pub(crate) vtimer_exits: u64,
    /// Trace events dropped because the ring was full.
    pub(crate) trace_overflow: u64,
}

/// Why a run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Every requested iteration completed.
    Completed,
    /// The guest stopped making progress and the deadline expired.
    Stalled,
    /// An unexpected guest exception or exit reason ended the run.
    Faulted,
}

impl Outcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Outcome::Completed => "completed",
            Outcome::Stalled => "stalled",
            Outcome::Faulted => "faulted",
        }
    }
}

/// A reason the run failed its criteria. Empty means pass.
pub(crate) fn failures(counters: &Counters, outcome: Outcome) -> Vec<String> {
    let mut out = Vec::new();
    if outcome != Outcome::Completed {
        out.push(format!("run outcome was {}", outcome.as_str()));
    }
    if counters.timer_wakes != counters.iterations {
        out.push(format!(
            "timer_wakes {} != iterations {}",
            counters.timer_wakes, counters.iterations
        ));
    }
    // A swallowed fire that recovery did not rescue is the A1 stall condition.
    if counters.masked_past_deadline > counters.recoveries {
        out.push(format!(
            "swallowed_unrecovered {}",
            counters.masked_past_deadline - counters.recoveries
        ));
    }
    if counters.trace_overflow != 0 {
        out.push(format!("trace_overflow {}", counters.trace_overflow));
    }
    if counters.vtimer_exits != 0 {
        out.push(format!(
            "vtimer_exits {} (in-kernel GIC delivery expected)",
            counters.vtimer_exits
        ));
    }
    out
}

/// Fires that were swallowed and never recovered.
pub(crate) fn swallowed_unrecovered(counters: &Counters) -> u64 {
    counters
        .masked_past_deadline
        .saturating_sub(counters.recoveries)
}

/// Render the receipt as JSON without pulling in a serializer.
pub(crate) fn to_json(counters: &Counters, outcome: Outcome, elapsed_ms: u128) -> String {
    let failures = failures(counters, outcome);
    let failure_list = failures
        .iter()
        .map(|f| format!("{:?}", f))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        concat!(
            "{{\n",
            "  \"probe\": \"hvf_vtimer_cancel\",\n",
            "  \"iterations\": {},\n",
            "  \"timer_wakes\": {},\n",
            "  \"canceled_exits\": {},\n",
            "  \"surplus_canceled\": {},\n",
            "  \"masked_past_deadline\": {},\n",
            "  \"recoveries\": {},\n",
            "  \"swallowed_unrecovered\": {},\n",
            "  \"vtimer_exits\": {},\n",
            "  \"trace_overflow\": {},\n",
            "  \"outcome\": \"{}\",\n",
            "  \"elapsed_ms\": {},\n",
            "  \"pass\": {},\n",
            "  \"failures\": [{}]\n",
            "}}"
        ),
        counters.iterations,
        counters.timer_wakes,
        counters.canceled_exits,
        counters.surplus_canceled,
        counters.masked_past_deadline,
        counters.recoveries,
        swallowed_unrecovered(counters),
        counters.vtimer_exits,
        counters.trace_overflow,
        outcome.as_str(),
        elapsed_ms,
        failures.is_empty(),
        failure_list,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(iterations: u64) -> Counters {
        Counters {
            iterations,
            timer_wakes: iterations,
            ..Counters::default()
        }
    }

    #[test]
    fn a_complete_run_with_every_wake_delivered_passes() {
        assert!(failures(&clean(10_000), Outcome::Completed).is_empty());
    }

    #[test]
    fn cancellation_pressure_alone_is_not_a_failure() {
        // The probe's purpose is to race cancels against fires. Cancels and
        // even surplus cancels are the stimulus, not the defect; only a lost
        // wake is.
        let counters = Counters {
            canceled_exits: 40_000,
            surplus_canceled: 900,
            ..clean(10_000)
        };
        assert!(failures(&counters, Outcome::Completed).is_empty());
    }

    #[test]
    fn a_swallowed_fire_that_recovery_rescued_passes() {
        let counters = Counters {
            masked_past_deadline: 37,
            recoveries: 37,
            ..clean(10_000)
        };
        assert!(failures(&counters, Outcome::Completed).is_empty());
        assert_eq!(swallowed_unrecovered(&counters), 0);
    }

    #[test]
    fn a_swallowed_fire_that_recovery_missed_fails() {
        let counters = Counters {
            masked_past_deadline: 37,
            recoveries: 36,
            ..clean(10_000)
        };
        let found = failures(&counters, Outcome::Completed);
        assert_eq!(swallowed_unrecovered(&counters), 1);
        assert!(
            found.iter().any(|f| f.contains("swallowed_unrecovered 1")),
            "{found:?}"
        );
    }

    #[test]
    fn a_missing_wake_fails_even_when_the_run_completes() {
        let counters = Counters {
            timer_wakes: 9_999,
            ..clean(10_000)
        };
        let found = failures(&counters, Outcome::Completed);
        assert!(found.iter().any(|f| f.contains("timer_wakes")), "{found:?}");
    }

    #[test]
    fn a_stalled_run_fails_even_with_matching_counters() {
        // The stall this probe exists to catch leaves the guest parked with
        // its deadline in the past, so "stopped early" must never pass.
        let found = failures(&clean(10_000), Outcome::Stalled);
        assert!(found.iter().any(|f| f.contains("stalled")), "{found:?}");
    }

    #[test]
    fn a_faulted_run_fails() {
        let found = failures(&clean(10_000), Outcome::Faulted);
        assert!(found.iter().any(|f| f.contains("faulted")), "{found:?}");
    }

    #[test]
    fn dropped_trace_events_fail_because_evidence_would_be_incomplete() {
        let counters = Counters {
            trace_overflow: 3,
            ..clean(10_000)
        };
        let found = failures(&counters, Outcome::Completed);
        assert!(
            found.iter().any(|f| f.contains("trace_overflow")),
            "{found:?}"
        );
    }

    #[test]
    fn an_unexpected_vtimer_exit_fails() {
        let counters = Counters {
            vtimer_exits: 1,
            ..clean(10_000)
        };
        let found = failures(&counters, Outcome::Completed);
        assert!(
            found.iter().any(|f| f.contains("vtimer_exits")),
            "{found:?}"
        );
    }

    #[test]
    fn the_receipt_reports_pass_and_the_derived_field() {
        let json = to_json(&clean(10), Outcome::Completed, 42);
        assert!(json.contains("\"pass\": true"), "{json}");
        assert!(json.contains("\"swallowed_unrecovered\": 0"), "{json}");
        assert!(json.contains("\"failures\": []"), "{json}");
        assert!(json.contains("\"elapsed_ms\": 42"), "{json}");
    }

    #[test]
    fn the_receipt_reports_failures_verbatim() {
        let counters = Counters {
            timer_wakes: 5,
            masked_past_deadline: 2,
            ..clean(10)
        };
        let json = to_json(&counters, Outcome::Stalled, 7);
        assert!(json.contains("\"pass\": false"), "{json}");
        assert!(json.contains("\"swallowed_unrecovered\": 2"), "{json}");
        assert!(json.contains("timer_wakes 5 != iterations 10"), "{json}");
        assert!(json.contains("stalled"), "{json}");
    }

    #[test]
    fn recoveries_beyond_swallowed_fires_do_not_underflow() {
        let counters = Counters {
            masked_past_deadline: 1,
            recoveries: 5,
            ..clean(10)
        };
        assert_eq!(swallowed_unrecovered(&counters), 0);
        assert!(failures(&counters, Outcome::Completed).is_empty());
    }
}
