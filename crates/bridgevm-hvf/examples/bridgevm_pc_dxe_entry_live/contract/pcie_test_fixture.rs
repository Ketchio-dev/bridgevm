use super::*;

pub(super) fn fixture() -> Vec<u8> {
    let mut result = vec![0; RESULT_OFFSET + 128];
    result[RESULT_OFFSET..RESULT_OFFSET + 4]
        .copy_from_slice(&(FUNCTION_COUNT as u32).to_le_bytes());
    for (index, identity) in identities::expected().into_iter().enumerate() {
        let offset = RESULT_OFFSET + 4 + index * 4;
        result[offset..offset + 4].copy_from_slice(&identity.to_le_bytes());
    }
    result[RESULT_OFFSET + 36..RESULT_OFFSET + 40].copy_from_slice(&1u32.to_le_bytes());
    result[RESULT_OFFSET + 40..RESULT_OFFSET + 44].copy_from_slice(&1u32.to_le_bytes());
    result[RESULT_OFFSET + 44..RESULT_OFFSET + 48].copy_from_slice(&2u32.to_le_bytes());
    nvme::write_fixture(&mut result);
    nvme_block::write_fixture(&mut result);
    result
}
