use super::*;

#[test]
fn accepts_all_versioned_endpoint_identities() {
    assert_eq!(
        validate(&test_fixture::fixture()).unwrap(),
        PcieProof {
            identities: identities::expected(),
            root_bridge_count: 1,
            enumeration_complete: 1,
            driver_binding_count: 2,
            supported_status: 0,
            connect_status: 0,
            nvme: nvme::fixture_proof(),
            nvme_block: nvme_block::fixture_proof(),
        }
    );
}
#[test]
fn rejects_one_mismatched_firmware_identity() {
    let mut result = test_fixture::fixture();
    result[RESULT_OFFSET + 4 + 5 * 4] ^= 1;
    assert!(validate(&result).unwrap_err().contains("PCIe identity"));
}
