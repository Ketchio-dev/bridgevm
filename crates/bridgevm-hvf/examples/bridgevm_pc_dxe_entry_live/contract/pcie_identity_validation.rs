use super::{expect, identities, u32_at, FUNCTION_COUNT, RESULT_OFFSET};

pub(super) fn validate(result: &[u8]) -> Result<[u32; FUNCTION_COUNT], String> {
    let expected = identities::expected();
    let mut identities = [0; FUNCTION_COUNT];
    for (index, identity) in identities.iter_mut().enumerate() {
        *identity = u32_at(result, RESULT_OFFSET + 4 + index * 4, "PCIe identity")?;
        expect("PCIe identity", *identity, expected[index])?;
    }
    // Retain the eight-slot result ABI without accepting ghost endpoints.
    for index in FUNCTION_COUNT..8 {
        expect(
            "reserved PCIe identity",
            u32_at(
                result,
                RESULT_OFFSET + 4 + index * 4,
                "reserved PCIe identity",
            )?,
            0,
        )?;
    }
    Ok(identities)
}
