use super::*;

use std::path::PathBuf;

fn scratch(tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let image = dir.join(format!("bv-sv-{tag}-{pid}.raw"));
    let receipt = dir.join(format!("bv-sv-{tag}-{pid}.receipt"));
    let _ = std::fs::remove_file(&receipt);
    std::fs::write(&image, b"image").unwrap();
    (image, receipt)
}

#[test]
fn each_cycle_sees_an_increasing_generation_and_its_own_pid() {
    let (image, receipt) = scratch("cycles");
    let mut next_pid = 100u32;
    let cycles = supervise_reset_cycles(
        |generation| {
            next_pid += 1;
            Ok(HelperExit {
                pid: next_pid,
                // Two resets, then a clean shutdown.
                reset_requested: generation < 2,
            })
        },
        &[&image],
        &receipt,
        100,
    )
    .unwrap();
    assert_eq!(
        cycles,
        vec![
            ResetCycle {
                pid: 101,
                generation: 0
            },
            ResetCycle {
                pid: 102,
                generation: 1
            },
            ResetCycle {
                pid: 103,
                generation: 2
            },
        ]
    );
}

#[test]
fn the_cycle_budget_bounds_a_guest_stuck_in_a_reset_loop() {
    let (image, receipt) = scratch("budget");
    let cycles = supervise_reset_cycles(
        |_| {
            Ok(HelperExit {
                pid: 7,
                reset_requested: true,
            })
        },
        &[&image],
        &receipt,
        5,
    )
    .unwrap();
    assert_eq!(cycles.len(), 5);
}

#[test]
fn a_helper_that_cannot_run_stops_the_loop_with_its_error() {
    let (image, receipt) = scratch("err");
    let error = supervise_reset_cycles(
        |_| {
            Err(RuntimeError::Io {
                context: "helper failed to launch",
                source: std::io::Error::other("spawn"),
            })
        },
        &[&image],
        &receipt,
        100,
    )
    .unwrap_err();
    assert!(error.to_string().contains("helper failed to launch"));
}

#[test]
fn a_missing_image_fails_the_flush_and_refuses_the_next_cycle() {
    let (image, receipt) = scratch("gone");
    std::fs::remove_file(&image).unwrap();
    let error = supervise_reset_cycles(
        |_| {
            Ok(HelperExit {
                pid: 7,
                reset_requested: true,
            })
        },
        &[&image],
        &receipt,
        100,
    )
    .unwrap_err();
    // The receipt was never written, so nothing claims the flush happened.
    assert!(!receipt.exists());
    assert!(
        error.to_string().contains("flush storage before reset"),
        "{error}"
    );
}
