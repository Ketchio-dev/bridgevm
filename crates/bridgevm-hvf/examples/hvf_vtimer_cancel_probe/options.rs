//! Command-line options for the vtimer/cancellation microprobe.
//!
//! Parsing is separated from the Hypervisor.framework code so the defaults and
//! the argument contract stay unit-testable.

use std::time::Duration;

/// Counter ticks the guest waits before its deadline. The architected counter
/// runs at 24 MHz on Apple silicon, so this is ~42 microseconds: short enough
/// that 10,000 iterations finish in about a second, long enough that the guest
/// reliably reaches `WFI` before the timer fires.
const DEFAULT_ARM_TICKS: u64 = 1_000;

/// Default gap between cancellations. Zero would spin a core at 100% and
/// starve the guest; this keeps cancels frequent relative to the ~42us arm
/// window so fires and cancels genuinely race.
const DEFAULT_CANCEL_INTERVAL: Duration = Duration::from_micros(37);

/// How long the guest may make no progress before the run is called stalled.
const DEFAULT_STALL_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Options {
    /// Timer wakes the guest must complete.
    pub(crate) iterations: u64,
    /// Counter ticks between arming and the deadline.
    pub(crate) arm_ticks: u64,
    /// Gap between host cancellations.
    pub(crate) cancel_interval: Duration,
    /// No-progress window before declaring a stall.
    pub(crate) stall_timeout: Duration,
    /// Whether to apply the swallowed-fire recovery. `--no-recover` turns it
    /// off, which is how the probe demonstrates it is reproducing a real
    /// stall rather than passing trivially.
    pub(crate) recover: bool,
    /// Optional path for the JSON receipt.
    pub(crate) receipt_path: Option<String>,
    /// When set, the first time the host observes the vtimer masked with its
    /// deadline already past, it stops all cancellation and runs the vCPU with
    /// only a long watchdog. That answers whether the swallowed fire is
    /// permanently lost or merely delayed, which no counter can distinguish.
    pub(crate) quiesce_probe: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            iterations: 10_000,
            arm_ticks: DEFAULT_ARM_TICKS,
            cancel_interval: DEFAULT_CANCEL_INTERVAL,
            stall_timeout: DEFAULT_STALL_TIMEOUT,
            recover: true,
            receipt_path: None,
            quiesce_probe: false,
        }
    }
}

pub(crate) const USAGE: &str = "\
usage: hvf_vtimer_cancel_probe [options]

  --iterations N        timer wakes the guest must complete (default 10000)
  --arm-ticks N         counter ticks from arm to deadline (default 1000)
  --cancel-interval-us N  microseconds between cancellations (default 37)
  --stall-timeout-ms N  no-progress window before failing (default 5000)
  --no-recover          disable swallowed-fire recovery (expected to stall)
  --quiesce-probe       on the first swallowed fire, halt cancellation and
                        report whether the wake still arrives
  --receipt PATH        write the JSON receipt to PATH
  -h, --help            print this message";

impl Options {
    pub(crate) fn parse<I: Iterator<Item = String>>(args: I) -> Result<Self, String> {
        let mut options = Options::default();
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            let mut value = || {
                args.next()
                    .ok_or_else(|| format!("{arg} requires a value\n\n{USAGE}"))
            };
            match arg.as_str() {
                "--iterations" => options.iterations = parse_u64(&value()?, "--iterations")?,
                "--arm-ticks" => options.arm_ticks = parse_u64(&value()?, "--arm-ticks")?,
                "--cancel-interval-us" => {
                    let micros = parse_u64(&value()?, "--cancel-interval-us")?;
                    options.cancel_interval = Duration::from_micros(micros);
                }
                "--stall-timeout-ms" => {
                    let millis = parse_u64(&value()?, "--stall-timeout-ms")?;
                    options.stall_timeout = Duration::from_millis(millis);
                }
                "--no-recover" => options.recover = false,
                "--quiesce-probe" => options.quiesce_probe = true,
                "--receipt" => options.receipt_path = Some(value()?),
                "-h" | "--help" => return Err(USAGE.to_string()),
                other => return Err(format!("unknown argument {other}\n\n{USAGE}")),
            }
        }
        if options.iterations == 0 {
            return Err("--iterations must be greater than zero".to_string());
        }
        if options.arm_ticks == 0 {
            return Err("--arm-ticks must be greater than zero".to_string());
        }
        Ok(options)
    }
}

fn parse_u64(value: &str, flag: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("{flag} expects a non-negative integer, got {value:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(args.iter().map(|a| a.to_string()))
    }

    #[test]
    fn the_default_run_is_the_ten_thousand_iteration_gate() {
        let options = parse(&[]).unwrap();
        assert_eq!(options.iterations, 10_000);
        assert!(options.recover, "recovery is on unless explicitly disabled");
        assert_eq!(options.receipt_path, None);
    }

    #[test]
    fn every_knob_can_be_set() {
        let options = parse(&[
            "--iterations",
            "25",
            "--arm-ticks",
            "500",
            "--cancel-interval-us",
            "9",
            "--stall-timeout-ms",
            "250",
            "--no-recover",
            "--receipt",
            "/tmp/r.json",
        ])
        .unwrap();
        assert_eq!(options.iterations, 25);
        assert_eq!(options.arm_ticks, 500);
        assert_eq!(options.cancel_interval, Duration::from_micros(9));
        assert_eq!(options.stall_timeout, Duration::from_millis(250));
        assert!(!options.recover);
        assert_eq!(options.receipt_path.as_deref(), Some("/tmp/r.json"));
    }

    #[test]
    fn a_zero_iteration_run_is_rejected_rather_than_passing_vacuously() {
        let error = parse(&["--iterations", "0"]).unwrap_err();
        assert!(error.contains("greater than zero"), "{error}");
    }

    #[test]
    fn a_zero_arm_window_is_rejected() {
        // A zero delta would put the deadline in the past before the guest
        // reached WFI, so every iteration would trivially "wake".
        let error = parse(&["--arm-ticks", "0"]).unwrap_err();
        assert!(error.contains("greater than zero"), "{error}");
    }

    #[test]
    fn a_zero_cancel_interval_is_allowed_as_maximum_pressure() {
        let options = parse(&["--cancel-interval-us", "0"]).unwrap();
        assert!(options.cancel_interval.is_zero());
    }

    #[test]
    fn the_quiesce_probe_is_off_unless_requested() {
        assert!(!parse(&[]).unwrap().quiesce_probe);
        assert!(parse(&["--quiesce-probe"]).unwrap().quiesce_probe);
    }

    #[test]
    fn a_missing_value_names_the_flag_that_needed_one() {
        let error = parse(&["--iterations"]).unwrap_err();
        assert!(error.contains("--iterations requires a value"), "{error}");
    }

    #[test]
    fn a_non_numeric_value_is_reported_with_the_offending_text() {
        let error = parse(&["--arm-ticks", "soon"]).unwrap_err();
        assert!(error.contains("--arm-ticks"), "{error}");
        assert!(error.contains("soon"), "{error}");
    }

    #[test]
    fn an_unknown_argument_is_rejected_rather_than_silently_ignored() {
        let error = parse(&["--iteration", "10"]).unwrap_err();
        assert!(error.contains("unknown argument --iteration"), "{error}");
    }

    #[test]
    fn help_returns_the_usage_text() {
        assert_eq!(parse(&["--help"]).unwrap_err(), USAGE);
        assert_eq!(parse(&["-h"]).unwrap_err(), USAGE);
    }
}
