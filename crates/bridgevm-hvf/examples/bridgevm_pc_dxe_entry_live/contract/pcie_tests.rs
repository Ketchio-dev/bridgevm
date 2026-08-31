use super::*;

fn fixture() -> Vec<u8> {
    let mut result = vec![0; RESULT_OFFSET + 56];
    result[RESULT_OFFSET..RESULT_OFFSET + 4]
        .copy_from_slice(&(FUNCTION_COUNT as u32).to_le_bytes());
    for (index, identity) in expected_identities().into_iter().enumerate() {
        let offset = RESULT_OFFSET + 4 + index * 4;
        result[offset..offset + 4].copy_from_slice(&identity.to_le_bytes());
    }
    result[RESULT_OFFSET + 36..RESULT_OFFSET + 40].copy_from_slice(&1u32.to_le_bytes());
    result[RESULT_OFFSET + 40..RESULT_OFFSET + 44].copy_from_slice(&1u32.to_le_bytes());
    result[RESULT_OFFSET + 44..RESULT_OFFSET + 48].copy_from_slice(&1u32.to_le_bytes());
    result
}

#[test]
fn accepts_all_versioned_endpoint_identities() {
    assert_eq!(
        validate(&fixture()).unwrap(),
        PcieProof {
            identities: expected_identities(),
            root_bridge_count: 1,
            enumeration_complete: 1,
            driver_binding_count: 1,
            supported_status: 0,
            connect_status: 0,
        }
    );
}

#[test]
fn rejects_one_mismatched_firmware_identity() {
    let mut result = fixture();
    result[RESULT_OFFSET + 4 + 5 * 4] ^= 1;
    assert!(validate(&result).unwrap_err().contains("PCIe identity"));
}
