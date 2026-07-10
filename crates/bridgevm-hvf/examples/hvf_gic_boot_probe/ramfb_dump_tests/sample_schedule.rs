use std::time::Duration;

use super::*;

#[test]
fn ramfb_sample_default_schedule_emits_each_label_once_when_elapsed() {
    // Given: the task-owned default RAMFB sample schedule.
    let mut schedule = RamfbSampleSchedule::default();
    let mut labels = Vec::new();

    // When: the probe loop reports elapsed time before, at, and after all checkpoints.
    schedule.emit_due(Duration::from_millis(999), |label| {
        labels.push(label.to_string());
    });
    schedule.emit_due(Duration::from_millis(1_000), |label| {
        labels.push(label.to_string());
    });
    schedule.emit_due(Duration::from_millis(5_000), |label| {
        labels.push(label.to_string());
    });
    schedule.emit_due(Duration::from_millis(15_000), |label| {
        labels.push(label.to_string());
    });
    schedule.emit_due(Duration::from_millis(30_000), |label| {
        labels.push(label.to_string());
    });

    // Then: every symmetric no-input/setup-input sample label appears exactly once.
    println!("sample labels: {}", labels.join(","));
    assert_eq!(
        labels,
        [
            "ramfb-sample-1000ms",
            "ramfb-sample-5000ms",
            "ramfb-sample-15000ms"
        ]
    );
}

#[test]
fn ramfb_sample_due_gate_closes_again_after_checkpoint_emission() {
    let mut schedule = RamfbSampleSchedule::from_millis_values(&[1_000, 5_000]).unwrap();

    assert!(!schedule.has_due_checkpoint(Duration::from_millis(999)));
    assert!(schedule.has_due_checkpoint(Duration::from_millis(1_000)));
    schedule.emit_due(Duration::from_millis(1_000), |_| {});
    assert!(!schedule.has_due_checkpoint(Duration::from_millis(1_001)));
    assert!(schedule.has_due_checkpoint(Duration::from_millis(5_000)));
}

#[test]
fn ramfb_sample_default_shell_observation_stops_without_proof_mode() {
    // Given: a default RAMFB sample schedule has not completed.
    let schedule = RamfbSampleSchedule::from_millis_values(&[1_000, 5_000]).unwrap();

    // When/Then: reaching the shell without proof mode preserves the historical stop behavior.
    assert_eq!(
        schedule.uefi_shell_observation(false, false),
        RamfbShellObservation::StopNow {
            reason: "serial reached UEFI shell"
        }
    );
}

#[test]
fn ramfb_sample_until_complete_continues_after_early_shell_observation() {
    // Given: proof mode is enabled and only the first requested RAMFB sample has emitted.
    let mut schedule = RamfbSampleSchedule::from_millis_values(&[1_000, 5_000]).unwrap();
    schedule.emit_due(Duration::from_millis(1_000), |_| {});

    // When/Then: reaching the shell before sample completion is an observation, not a stop.
    assert_eq!(
        schedule.uefi_shell_observation(true, false),
        RamfbShellObservation::ContinueSampling {
            message: "serial reached UEFI shell before RAMFB sample schedule complete"
        }
    );

    // When: the remaining requested sample has emitted after the shell observation.
    schedule.emit_due(Duration::from_millis(5_000), |_| {});

    // Then: the probe can stop with a reason that distinguishes proof-mode completion.
    assert_eq!(
        schedule.uefi_shell_observation(true, true),
        RamfbShellObservation::StopNow {
            reason: "ramfb sample schedule complete after UEFI shell"
        }
    );
}

#[test]
fn ramfb_sample_env_rejects_invalid_paths_without_stale_triggers() {
    // Given: malformed sample schedule values from the env boundary.
    let cases = [
        (" ", RamfbSampleEnvError::Empty),
        (
            "1000,nope",
            RamfbSampleEnvError::Invalid {
                token: "nope".into(),
            },
        ),
        (
            "1000,1000",
            RamfbSampleEnvError::Duplicate { sample_ms: 1000 },
        ),
        (
            "120001",
            RamfbSampleEnvError::TooLarge {
                requested_ms: 120_001,
                max_ms: 120_000,
            },
        ),
        (
            "1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17",
            RamfbSampleEnvError::TooMany {
                requested: 17,
                max: 16,
            },
        ),
    ];

    // When/Then: each invalid input is rejected before a sampler can emit stale labels.
    for (value, expected) in cases {
        let error = RamfbSampleSchedule::from_env_value(value).unwrap_err();
        println!("sample parse rejection: {}", error.name());
        assert_eq!(error, expected);
    }
}
