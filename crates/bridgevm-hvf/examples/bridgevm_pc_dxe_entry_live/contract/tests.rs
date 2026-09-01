use super::*;
#[test]
fn rejects_non_contract_firmware_size() {
    assert!(validate_firmware(&[0; 64]).unwrap_err().contains("FD size"));
}
#[test]
fn rejects_a_missing_dxe_dispatch_result() {
    let ram = vec![0; 0x80_0000];
    assert!(validate_dxe_result(&ram, VariableState::Written)
        .unwrap_err()
        .contains("SEC stage"));
}
