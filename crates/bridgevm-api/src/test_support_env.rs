//! Environment fixtures for tests that share process-global configuration.

use std::sync::{Mutex, MutexGuard};

const POWER_STATE_KEY: &str = "BRIDGEVM_FORCE_ON_BATTERY";
static POWER_STATE_ENV_LOCK: Mutex<()> = Mutex::new(());

pub(crate) struct EnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    pub(crate) fn capture(key: &'static str) -> Self {
        Self {
            key,
            previous: std::env::var_os(key),
        }
    }

    pub(crate) fn set(key: &'static str, value: &str) -> Self {
        let guard = Self::capture(key);
        std::env::set_var(key, value);
        guard
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

pub(crate) struct PowerStateGuard {
    // Fields drop in declaration order: restore the value before releasing the lock.
    _restore: EnvVarGuard,
    _lock: MutexGuard<'static, ()>,
}

impl PowerStateGuard {
    pub(crate) fn set(on_battery: bool) -> Self {
        let lock = POWER_STATE_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Self {
            _restore: EnvVarGuard::set(POWER_STATE_KEY, if on_battery { "1" } else { "0" }),
            _lock: lock,
        }
    }
}

#[test]
fn runtime_power_override_excludes_overlapping_guards_and_restores_the_previous_value() {
    let ac = PowerStateGuard::set(false);
    let previous = ac._restore.previous.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let contender = std::thread::spawn(move || {
        ready_tx
            .send(matches!(
                POWER_STATE_ENV_LOCK.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ))
            .unwrap();
        let _battery = PowerStateGuard::set(true);
        assert!(bridgevm_resource_manager::read_on_battery());
    });
    assert!(ready_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap());
    assert!(!bridgevm_resource_manager::read_on_battery());
    drop(ac);
    contender.join().unwrap();

    let _lock = POWER_STATE_ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(std::env::var_os(POWER_STATE_KEY), previous);
}

#[test]
fn runtime_power_overrides_preserve_ac_and_battery_reads_across_parallel_tests() {
    let start = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        for on_battery in [false, true] {
            let start = &start;
            scope.spawn(move || {
                start.wait();
                for _ in 0..32 {
                    let _power = PowerStateGuard::set(on_battery);
                    for _ in 0..32 {
                        assert_eq!(bridgevm_resource_manager::read_on_battery(), on_battery);
                        std::thread::yield_now();
                    }
                }
            });
        }
    });
}
