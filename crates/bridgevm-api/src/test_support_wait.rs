//! Waiting and env-lock helpers shared by the process tests.
//!
//! Split out of test_support.rs.

use super::*;

/// Poll `condition` for ten seconds, returning whether it ever held.
///
/// A bare `for _ in 0..200` loop that just falls through on timeout turns a
/// slow spawn into a confusing failure much later -- one such loop reported
/// itself as a NotFound from an unrelated `read_to_string`, which then poisoned
/// the env mutex and failed three more tests.
///
/// Ten seconds rather than two because these wait on a spawned helper process,
/// and two was not enough under an eight-way parallel test load. The budget only
/// bounds a failure; a healthy spawn returns in milliseconds.
pub(crate) fn wait_up_to_ten_seconds(mut condition: impl FnMut() -> bool) -> bool {
    for _ in 0..1000 {
        if condition() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

/// Take the env lock, ignoring poisoning.
///
/// The mutex only orders access to one environment variable. When a test
/// panicked while holding it, every later test that touched the same variable
/// failed with PoisonError instead of its own result -- one real failure was
/// reported as four. Poisoning carries no information here, because the guarded
/// data is `()` and each test sets the variable it needs before reading it.
pub(crate) fn lock_apple_vz_runner_env() -> std::sync::MutexGuard<'static, ()> {
    APPLE_VZ_RUNNER_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod lock_poisoning {
    use super::*;

    #[test]
    fn a_panic_elsewhere_does_not_fail_later_env_tests() {
        // A test that panics while holding this mutex used to make every later
        // test touching the same env var fail with PoisonError instead of its
        // own result: one real failure was reported as four.
        let _ = std::panic::catch_unwind(|| {
            let _held = lock_apple_vz_runner_env();
            panic!("a test panicked while holding the env lock");
        });
        assert!(APPLE_VZ_RUNNER_ENV_LOCK.is_poisoned());

        // The next taker must still get the lock.
        let _guard = lock_apple_vz_runner_env();
    }
}
