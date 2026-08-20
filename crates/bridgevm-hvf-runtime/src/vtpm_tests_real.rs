//! Tests that need a real swtpm binary on the host.
//!
//! Split out of vtpm_tests.rs, which keeps the ones that do not.

use super::super::*;
use super::{scratch_state, wait_for_state_file};
use std::path::PathBuf;

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
fn a_binary_that_dies_just_after_binding_its_sockets_is_not_trusted() {
    // swtpm binds its sockets before it decrypts the state directory, so a
    // wrong key produces a process that briefly looks healthy and then exits.
    // Socket existence alone handed back a live-looking handle to a process
    // that could not answer swtpm's readiness protocol. This reproduces that
    // sequence without relying on a scheduler-sensitive grace period.
    let dir = std::env::temp_dir().join(format!("bv-vtpm-flashbin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("flash-swtpm.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\n\
         # Bind both sockets the way swtpm does, then exit like a bad key.\n\
         for arg in \"$@\"; do\n\
         \x20 case \"$arg\" in\n\
         \x20   *path=*) p=${arg#*path=}; p=${p%%,*}; nc -lU \"$p\" >/dev/null 2>&1 &\n\
         \x20   ;;\n\
         \x20 esac\n\
         done\n\
         sleep 0.05\n\
         exit 1\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let outcome = start_swtpm(&VtpmConfig {
        state_dir: scratch_state("flash"),
        swtpm_bin: script,
        state_key: None,
    });
    let _ = std::fs::remove_dir_all(&dir);

    let error = match outcome {
        Err(error) => error,
        Ok(_) => panic!("a process that exits right after binding must not be trusted"),
    };
    assert!(
        error.to_string().contains("sockets"),
        "the error must name the socket wait: {error}"
    );
}

#[test]
fn runtime_directory_names_cannot_collide_within_a_process() {
    // The name was pid plus subsec_nanos, which only distinguishes within one
    // second: 177,519 collisions in 200,000 draws. Two instances that collided
    // shared a runtime directory, so dropping either one deleted the other's
    // live sockets while it was still serving them. A rapid burst is exactly
    // the case that broke, so generate one rather than sampling slowly.
    let names: std::collections::HashSet<String> =
        (0..200_000).map(|_| unique_runtime_dir_name()).collect();
    assert_eq!(
        names.len(),
        200_000,
        "runtime directory names collided; two swtpm instances would share one"
    );

    // The sockets live inside this directory and a Unix socket path is capped
    // at 104 bytes. A nanosecond timestamp in the name reached 103 on a macOS
    // temp dir, which broke swtpm as soon as the counter needed two digits.
    let longest = std::env::temp_dir()
        .join(unique_runtime_dir_name())
        .join("control.sock");
    assert!(
        longest.as_os_str().len() < 104,
        "socket path {} is {} bytes; the Unix limit is 104",
        longest.display(),
        longest.as_os_str().len()
    );
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
    let persisted = wait_for_state_file(&state.join("tpm2-00.permall"));
    drop(first);
    assert!(persisted, "keyed state must persist");
    // Same key: opens.
    let same = start_swtpm(&VtpmConfig {
        state_dir: state.clone(),
        swtpm_bin: swtpm.clone(),
        state_key: Some(key),
    });
    assert!(same.is_ok(), "the right key must open the state");
    drop(same);
    // Wrong key: swtpm must fail, not serve sockets over unreadable state.
    // Dropping SIGKILLs swtpm, so confirm the encrypted state actually survived
    // before concluding anything from the refusal -- against an absent state
    // file any key "works", which would make this assertion vacuous.
    assert!(
        state.join("tpm2-00.permall").exists(),
        "the encrypted state must still exist for the wrong key to be refused"
    );
    let wrong = start_swtpm(&VtpmConfig {
        state_dir: state.clone(),
        swtpm_bin: swtpm,
        state_key: Some(vec![0x24u8; 32]),
    });
    assert!(wrong.is_err(), "the wrong key must be refused");
    let _ = std::fs::remove_dir_all(&state);
}
