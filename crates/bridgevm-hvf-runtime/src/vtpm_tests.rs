//! The swtpm lifecycle: real process when the binary exists, honest
//! failures when it does not.

use super::*;

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

#[test]
fn a_real_swtpm_serves_sockets_and_dies_with_the_handle() {
    let swtpm = PathBuf::from("/opt/homebrew/bin/swtpm");
    if !swtpm.exists() {
        eprintln!("skipping: no swtpm on this host");
        return;
    }
    let state = scratch_state("real");
    let process = start_swtpm(&VtpmConfig {
        state_dir: state.clone(),
        swtpm_bin: swtpm,
        state_key: None,
    })
    .expect("swtpm starts");
    assert!(process.data_socket().exists());
    assert!(process.control_socket().exists());
    let runtime_dir = process.data_socket().parent().unwrap().to_path_buf();
    drop(process);
    assert!(
        !runtime_dir.exists(),
        "drop must remove the socket directory"
    );
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
fn an_encrypted_state_dir_refuses_the_wrong_key() {
    let swtpm = PathBuf::from("/opt/homebrew/bin/swtpm");
    if !swtpm.exists() {
        eprintln!("skipping: no swtpm on this host");
        return;
    }
    let state = scratch_state("keyed");
    let key = vec![0x42u8; 32];
    // First run creates state under the key.
    let first = start_swtpm(&VtpmConfig {
        state_dir: state.clone(),
        swtpm_bin: swtpm.clone(),
        state_key: Some(key.clone()),
    })
    .expect("keyed swtpm starts");
    drop(first);
    assert!(
        state.join("tpm2-00.permall").exists(),
        "keyed state must persist"
    );
    // Same key: opens.
    let same = start_swtpm(&VtpmConfig {
        state_dir: state.clone(),
        swtpm_bin: swtpm.clone(),
        state_key: Some(key),
    });
    assert!(same.is_ok(), "the right key must open the state");
    drop(same);
    // Wrong key: swtpm must fail, not serve sockets over unreadable state.
    let wrong = start_swtpm(&VtpmConfig {
        state_dir: state.clone(),
        swtpm_bin: swtpm,
        state_key: Some(vec![0x24u8; 32]),
    });
    assert!(wrong.is_err(), "the wrong key must be refused");
    let _ = std::fs::remove_dir_all(&state);
}
