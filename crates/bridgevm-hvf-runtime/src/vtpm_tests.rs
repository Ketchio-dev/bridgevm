//! The swtpm lifecycle: real process when the binary exists, honest
//! failures when it does not.

use super::*;

/// `start_swtpm` waits for the sockets, which is all the product needs, but
/// swtpm writes its state file separately and slightly later. Dropping the
/// handle SIGKILLs the process, so a state file it has not written by then is
/// never written at all -- wait while it is still alive, not after. Asserting
/// immediately passed on an idle machine 10 times out of 10 and failed under an
/// eight-way parallel test load.
fn wait_for_state_file(path: &Path) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}

fn scratch_state(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bv-vtpm-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a_missing_swtpm_binary_names_the_spawn_context() {
    let error = match start_swtpm(&VtpmConfig {
        state_dir: scratch_state("missing"),
        swtpm_bin: "/nonexistent/swtpm".into(),
        state_key: None,
    }) {
        Err(error) => error,
        Ok(_) => panic!("no binary must not start"),
    };
    assert!(error.to_string().contains("spawn swtpm"), "{error}");
}

#[test]
fn a_binary_that_exits_early_is_reported_not_waited_out() {
    // /usr/bin/false accepts any args and exits 1 immediately: the wait
    // loop must report the death, not sleep to the timeout.
    let started = std::time::Instant::now();
    let error = match start_swtpm(&VtpmConfig {
        state_dir: scratch_state("early"),
        swtpm_bin: "/usr/bin/false".into(),
        state_key: None,
    }) {
        Err(error) => error,
        Ok(_) => panic!("false must not serve sockets"),
    };
    assert!(
        error.to_string().contains("before creating its sockets"),
        "{error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "must fail fast on exit, not wait for the timeout"
    );
}

#[path = "vtpm_tests_real.rs"]
mod real;
