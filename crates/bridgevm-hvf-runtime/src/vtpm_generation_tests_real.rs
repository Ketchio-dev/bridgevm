//! Two helper generations against the run's one real swtpm, connected through
//! the probe's boot setup (`SwtpmUnixBackend::connect_for_boot`).

use super::super::*;
use super::scratch_state;
use bridgevm_hvf::tpm_tis::{SwtpmUnixBackend, Tpm2Backend};

const STARTUP_CLEAR: [u8; 12] = [0x80, 0x01, 0, 0, 0, 12, 0, 0, 0x01, 0x44, 0, 0];
/// TPM2_PCR_Read of PCR 0 in the SHA-256 bank.
const PCR_READ_0: [u8; 20] = [
    0x80, 0x01, 0, 0, 0, 20, 0, 0, 0x01, 0x7e, 0, 0, 0, 1, 0x00, 0x0b, 3, 1, 0, 0,
];

/// TPM2_PCR_Extend of PCR 0 with one SHA-256 digest, authorized by an empty
/// password session, as firmware measurements are.
fn pcr_extend_0() -> Vec<u8> {
    let mut command = vec![
        0x80, 0x02, 0, 0, 0, 65, 0, 0, 0x01, 0x82, 0, 0, 0, 0, 0, 0, 0, 9,
    ];
    command.extend_from_slice(&[0x40, 0, 0, 0x09, 0, 0, 0, 0, 0]);
    command.extend_from_slice(&[0, 0, 0, 1, 0x00, 0x0b]);
    command.extend_from_slice(&[0x11; 32]);
    command
}

fn response_code(backend: &mut SwtpmUnixBackend, command: &[u8]) -> (u32, Vec<u8>) {
    let response = backend.execute(0, command).expect("swtpm answers");
    let code = u32::from_be_bytes(response[6..10].try_into().unwrap());
    (code, response)
}

fn pcr0(backend: &mut SwtpmUnixBackend) -> Vec<u8> {
    let (code, response) = response_code(backend, &PCR_READ_0);
    assert_eq!(code, 0, "PCR_Read");
    // Header, update counter, selection, digest count, digest size.
    response[30..62].to_vec()
}

#[test]
fn a_new_helper_generation_boots_from_cleared_pcrs() {
    let swtpm = std::path::PathBuf::from("/opt/homebrew/bin/swtpm");
    if !swtpm.exists() {
        eprintln!("skipping: no swtpm on this host");
        return;
    }
    let state = scratch_state("generations");
    let process = start_swtpm(&VtpmConfig {
        state_dir: state.clone(),
        swtpm_bin: swtpm,
        state_key: None,
    })
    .expect("swtpm starts");
    let boot = || {
        SwtpmUnixBackend::connect_for_boot(process.data_socket(), Some(process.control_socket()))
            .expect("a generation connects")
    };

    // Generation 1: firmware's Startup(CLEAR), then one measurement.
    let mut first = boot();
    let (startup, _) = response_code(&mut first, &STARTUP_CLEAR);
    assert!(matches!(startup, 0 | 0x100), "Startup(CLEAR) {startup:#x}");
    assert_eq!(
        response_code(&mut first, &pcr_extend_0()).0,
        0,
        "PCR_Extend"
    );
    assert_ne!(pcr0(&mut first), [0; 32]);
    drop(first);

    // Generation 2, as after a guest reset: a new helper, the same swtpm.
    let mut second = boot();
    let (startup, _) = response_code(&mut second, &STARTUP_CLEAR);
    assert_eq!(
        pcr0(&mut second),
        [0; 32],
        "generation 2 inherited generation 1's PCR 0; Startup(CLEAR) returned {startup:#x}"
    );
    drop(second);
    drop(process);
    let _ = std::fs::remove_dir_all(&state);
}
