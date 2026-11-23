//! AML (ACPI Machine Language) opcodes and encoders, and the resource
//! descriptors the DSDT devices use.

pub(crate) const AML_ZERO_OP: u8 = 0x00;
pub(crate) const AML_ONE_OP: u8 = 0x01;
pub(crate) const AML_BYTE_PREFIX: u8 = 0x0A;
pub(crate) const AML_DWORD_PREFIX: u8 = 0x0C;
pub(crate) const AML_STRING_PREFIX: u8 = 0x0D;
pub(crate) const AML_NAME_OP: u8 = 0x08;
pub(crate) const AML_SCOPE_OP: u8 = 0x10;
pub(crate) const AML_BUFFER_OP: u8 = 0x11;
pub(crate) const AML_PACKAGE_OP: u8 = 0x12;
pub(crate) const AML_METHOD_OP: u8 = 0x14;
pub(crate) const AML_EXT_OP: u8 = 0x5B;
pub(crate) const AML_DEVICE_OP: u8 = 0x82;
pub(crate) const AML_OPERATION_REGION_OP: u8 = 0x80;
pub(crate) const AML_FIELD_OP: u8 = 0x81;
pub(crate) const AML_LOCAL0_OP: u8 = 0x60;
pub(crate) const AML_ARG0_OP: u8 = 0x68;
pub(crate) const AML_STORE_OP: u8 = 0x70;
pub(crate) const AML_ADD_OP: u8 = 0x72;
pub(crate) const AML_AND_OP: u8 = 0x7B;
pub(crate) const AML_OR_OP: u8 = 0x7D;
pub(crate) const AML_DEREF_OF_OP: u8 = 0x83;
pub(crate) const AML_NOTIFY_OP: u8 = 0x86;
pub(crate) const AML_INDEX_OP: u8 = 0x88;
pub(crate) const AML_CREATE_DWORD_FIELD_OP: u8 = 0x8A;
pub(crate) const AML_LNOT_OP: u8 = 0x92;
pub(crate) const AML_LEQUAL_OP: u8 = 0x93;
pub(crate) const AML_IF_OP: u8 = 0xA0;
pub(crate) const AML_ELSE_OP: u8 = 0xA1;
pub(crate) const AML_RETURN_OP: u8 = 0xA4;

pub(crate) const EISA_PNP0A08: [u8; 4] = [0x41, 0xD0, 0x0A, 0x08];
pub(crate) const EISA_PNP0A03: [u8; 4] = [0x41, 0xD0, 0x0A, 0x03];
pub(crate) const EISA_PNP0C02: [u8; 4] = [0x41, 0xD0, 0x0C, 0x02];
pub(crate) const EISA_PNP0C0C: [u8; 4] = [0x41, 0xD0, 0x0C, 0x0C];
pub(crate) const PCI_HOST_BRIDGE_OSC_UUID: [u8; 16] = [
    0x5B, 0x4D, 0xDB, 0x33, 0xF7, 0x1F, 0x1C, 0x40, 0x96, 0x57, 0x74, 0x41, 0xC0, 0x3D, 0xD7, 0x66,
];
pub(crate) const TPM_PPI_DSM_UUID: [u8; 16] = [
    0xA6, 0xFA, 0xDD, 0x3D, 0x1B, 0x36, 0xB4, 0x4E, 0xA4, 0x24, 0x8D, 0x10, 0x08, 0x9D, 0x16, 0x53,
];
pub(crate) const TPM_RESET_ATTACK_DSM_UUID: [u8; 16] = [
    0xED, 0x54, 0x60, 0x37, 0x13, 0xCC, 0x75, 0x46, 0x90, 0x1C, 0x47, 0x56, 0xD7, 0xF2, 0xD4, 0x5D,
];

pub(crate) fn aml_pkg_length(payload_len: usize) -> Vec<u8> {
    for encoded_len in 1..=4 {
        let total = payload_len + encoded_len;
        if encoded_len == 1 {
            if total <= 0x3F {
                return vec![total as u8];
            }
            continue;
        }
        let max = match encoded_len {
            2 => 0x0FFF,
            3 => 0x0F_FFFF,
            4 => 0x0FFF_FFFF,
            _ => unreachable!(),
        };
        if total <= max {
            let follow = encoded_len - 1;
            let mut out = Vec::with_capacity(encoded_len);
            out.push(((follow as u8) << 6) | ((total & 0x0F) as u8));
            let mut rest = total >> 4;
            for _ in 0..follow {
                out.push((rest & 0xFF) as u8);
                rest >>= 8;
            }
            return out;
        }
    }
    panic!("AML package length too large: {payload_len}");
}

pub(crate) fn aml_name_string(name: &[u8; 4], value: &str) -> Vec<u8> {
    assert!(
        !value.as_bytes().contains(&0),
        "AML strings are NUL-terminated"
    );
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.push(AML_STRING_PREFIX);
    out.extend_from_slice(value.as_bytes());
    out.push(0);
    out
}

pub(crate) fn aml_string(value: &str) -> Vec<u8> {
    assert!(
        !value.as_bytes().contains(&0),
        "AML strings are NUL-terminated"
    );
    let mut out = vec![AML_STRING_PREFIX];
    out.extend_from_slice(value.as_bytes());
    out.push(0);
    out
}

pub(crate) fn aml_name_eisa(name: &[u8; 4], encoded: [u8; 4]) -> Vec<u8> {
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.push(AML_DWORD_PREFIX);
    out.extend_from_slice(&encoded);
    out
}

pub(crate) fn aml_name_simple(name: &[u8; 4], op: u8) -> Vec<u8> {
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.push(op);
    out
}

pub(crate) fn aml_name_byte(name: &[u8; 4], value: u8) -> Vec<u8> {
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.extend(aml_byte(value));
    out
}

pub(crate) fn aml_name_buffer(name: &[u8; 4], bytes: &[u8]) -> Vec<u8> {
    assert!(bytes.len() <= u8::MAX as usize, "small AML buffer expected");
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.push(AML_BUFFER_OP);
    out.extend(aml_pkg_length(2 + bytes.len()));
    out.push(AML_BYTE_PREFIX);
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
    out
}

pub(crate) fn aml_buffer(bytes: &[u8]) -> Vec<u8> {
    assert!(bytes.len() <= u8::MAX as usize, "small AML buffer expected");
    let mut out = vec![AML_BUFFER_OP];
    out.extend(aml_pkg_length(2 + bytes.len()));
    out.push(AML_BYTE_PREFIX);
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
    out
}

pub(crate) fn aml_package(elements: &[Vec<u8>]) -> Vec<u8> {
    assert!(
        elements.len() <= u8::MAX as usize,
        "small AML package expected"
    );
    let payload_len = 1 + elements.iter().map(Vec::len).sum::<usize>();
    let mut out = vec![AML_PACKAGE_OP];
    out.extend(aml_pkg_length(payload_len));
    out.push(elements.len() as u8);
    for element in elements {
        out.extend_from_slice(element);
    }
    out
}

pub(crate) fn aml_name_package(name: &[u8; 4], elements: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![AML_NAME_OP];
    out.extend_from_slice(name);
    out.extend(aml_package(elements));
    out
}

pub(crate) fn aml_name_ref(name: &[u8; 4]) -> Vec<u8> {
    name.to_vec()
}

pub(crate) fn aml_arg(n: u8) -> Vec<u8> {
    assert!(n <= 6, "AML has only Arg0..Arg6");
    vec![AML_ARG0_OP + n]
}

pub(crate) fn aml_local0() -> Vec<u8> {
    vec![AML_LOCAL0_OP]
}

pub(crate) fn aml_local(n: u8) -> Vec<u8> {
    assert!(n <= 7, "AML has only Local0..Local7");
    vec![AML_LOCAL0_OP + n]
}

pub(crate) fn aml_byte(value: u8) -> Vec<u8> {
    match value {
        0 => vec![AML_ZERO_OP],
        1 => vec![AML_ONE_OP],
        _ => vec![AML_BYTE_PREFIX, value],
    }
}

pub(crate) fn aml_dword(value: u32) -> Vec<u8> {
    let mut out = vec![AML_DWORD_PREFIX];
    out.extend_from_slice(&value.to_le_bytes());
    out
}

pub(crate) fn aml_uuid_buffer(bytes: &[u8; 16]) -> Vec<u8> {
    let mut out = vec![AML_BUFFER_OP];
    out.extend(aml_pkg_length(2 + bytes.len()));
    out.push(AML_BYTE_PREFIX);
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
    out
}

pub(crate) fn aml_create_dword_field(source: &[u8], byte_index: u8, name: &[u8; 4]) -> Vec<u8> {
    let mut out = vec![AML_CREATE_DWORD_FIELD_OP];
    out.extend_from_slice(source);
    out.extend(aml_byte(byte_index));
    out.extend_from_slice(name);
    out
}

pub(crate) fn aml_store(source: &[u8], target: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_STORE_OP];
    out.extend_from_slice(source);
    out.extend_from_slice(target);
    out
}

pub(crate) fn aml_index(source: &[u8], index: &[u8], target: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_INDEX_OP];
    out.extend_from_slice(source);
    out.extend_from_slice(index);
    out.extend_from_slice(target);
    out
}

pub(crate) fn aml_deref_of(reference: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_DEREF_OF_OP];
    out.extend_from_slice(reference);
    out
}

pub(crate) fn aml_call1(name: &[u8; 4], arg: &[u8]) -> Vec<u8> {
    let mut out = name.to_vec();
    out.extend_from_slice(arg);
    out
}

pub(crate) fn aml_binary_op(op: u8, left: &[u8], right: &[u8], target: &[u8]) -> Vec<u8> {
    let mut out = vec![op];
    out.extend_from_slice(left);
    out.extend_from_slice(right);
    out.extend_from_slice(target);
    out
}

pub(crate) fn aml_equal(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_LEQUAL_OP];
    out.extend_from_slice(left);
    out.extend_from_slice(right);
    out
}

pub(crate) fn aml_not_equal(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_LNOT_OP];
    out.extend(aml_equal(left, right));
    out
}

pub(crate) fn aml_if(predicate: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_IF_OP];
    out.extend(aml_pkg_length(predicate.len() + body.len()));
    out.extend_from_slice(predicate);
    out.extend_from_slice(body);
    out
}

pub(crate) fn aml_else(body: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_ELSE_OP];
    out.extend(aml_pkg_length(body.len()));
    out.extend_from_slice(body);
    out
}

pub(crate) fn aml_return(value: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_RETURN_OP];
    out.extend_from_slice(value);
    out
}

pub(crate) fn aml_operation_region(name: &[u8; 4], base: &[u8], length: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_EXT_OP, AML_OPERATION_REGION_OP];
    out.extend_from_slice(name);
    out.push(0x00); // SystemMemory
    out.extend_from_slice(base);
    out.extend_from_slice(length);
    out
}

pub(crate) fn aml_field(name: &[u8; 4], flags: u8, fields: &[(&[u8; 4], usize)]) -> Vec<u8> {
    let mut field_list = Vec::new();
    for (field_name, bit_length) in fields {
        field_list.extend_from_slice(*field_name);
        field_list.extend(aml_field_length(*bit_length));
    }
    let mut out = vec![AML_EXT_OP, AML_FIELD_OP];
    out.extend(aml_pkg_length(name.len() + 1 + field_list.len()));
    out.extend_from_slice(name);
    out.push(flags);
    out.extend(field_list);
    out
}

pub(crate) fn aml_field_length(value: usize) -> Vec<u8> {
    if value <= 0x3f {
        return vec![value as u8];
    }
    assert!(value <= 0x0fff, "small AML field expected");
    vec![0x40 | (value as u8 & 0x0f), (value >> 4) as u8]
}

pub(crate) fn aml_scope(name: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_SCOPE_OP];
    out.extend(aml_pkg_length(name.len() + body.len()));
    out.extend_from_slice(name);
    out.extend_from_slice(body);
    out
}

pub(crate) fn aml_device(name: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_EXT_OP, AML_DEVICE_OP];
    out.extend(aml_pkg_length(name.len() + body.len()));
    out.extend_from_slice(name);
    out.extend_from_slice(body);
    out
}

pub(crate) fn aml_method(name: &[u8; 4], arg_count: u8, serialized: bool, body: &[u8]) -> Vec<u8> {
    assert!(
        arg_count <= 7,
        "AML methods support at most seven arguments"
    );
    let mut out = vec![AML_METHOD_OP];
    out.extend(aml_pkg_length(name.len() + 1 + body.len()));
    out.extend_from_slice(name);
    out.push(arg_count | if serialized { 0x08 } else { 0x00 });
    out.extend_from_slice(body);
    out
}

pub(crate) fn aml_notify(object: &[u8], value: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_NOTIFY_OP];
    out.extend_from_slice(object);
    out.extend_from_slice(value);
    out
}

pub(crate) fn resource_memory32_fixed(base: u64, size: u64) -> Vec<u8> {
    let base = u32::try_from(base).expect("Memory32Fixed base exceeds 32 bits");
    let size = u32::try_from(size).expect("Memory32Fixed size exceeds 32 bits");
    let mut out = vec![0x86, 0x09, 0x00, 0x01]; // Memory32Fixed, ReadWrite
    out.extend_from_slice(&base.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out
}

pub(crate) fn resource_interrupt(gsiv: u32) -> Vec<u8> {
    let mut out = vec![0x89, 0x06, 0x00, 0x01, 0x01]; // Consumer, level, active-high, exclusive
    out.extend_from_slice(&gsiv.to_le_bytes());
    out
}

pub(crate) fn resource_edge_interrupt(gsiv: u32) -> Vec<u8> {
    let mut out = resource_interrupt(gsiv);
    out[3] |= 0x02; // Edge-triggered
    out
}

pub(crate) fn resource_word_bus_number(min: u16, max: u16) -> Vec<u8> {
    let len = max
        .checked_sub(min)
        .and_then(|v| v.checked_add(1))
        .expect("invalid PCI bus range");
    let mut out = vec![0x88, 0x0D, 0x00, 0x02, 0x0C, 0x00]; // Word address, bus, min/max fixed
    out.extend_from_slice(&0u16.to_le_bytes()); // granularity
    out.extend_from_slice(&min.to_le_bytes());
    out.extend_from_slice(&max.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // translation offset
    out.extend_from_slice(&len.to_le_bytes());
    out
}

pub(crate) fn resource_dword_memory(base: u64, size: u64) -> Vec<u8> {
    let max = base
        .checked_add(size)
        .and_then(|value| value.checked_sub(1))
        .expect("DWordMemory range overflow");
    let base = u32::try_from(base).expect("DWordMemory base exceeds 32 bits");
    let size = u32::try_from(size).expect("DWordMemory size exceeds 32 bits");
    let max = u32::try_from(max).expect("DWordMemory maximum exceeds 32 bits");
    let mut out = vec![0x87, 0x17, 0x00, 0x00, 0x0C, 0x01]; // Memory, min/max fixed, read-write
    out.extend_from_slice(&0u32.to_le_bytes()); // granularity
    out.extend_from_slice(&base.to_le_bytes());
    out.extend_from_slice(&max.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // translation offset
    out.extend_from_slice(&size.to_le_bytes());
    out
}

pub(crate) fn resource_dword_io(base: u64, size: u64) -> Vec<u8> {
    let base = u32::try_from(base).expect("DWordIO translation exceeds 32 bits");
    let size = u32::try_from(size).expect("DWordIO size exceeds 32 bits");
    let max = size.checked_sub(1).expect("DWordIO size must be non-zero");
    let mut out = vec![0x87, 0x17, 0x00, 0x01, 0x0C, 0x03]; // I/O, min/max fixed, entire range
    out.extend_from_slice(&0u32.to_le_bytes()); // granularity
    out.extend_from_slice(&0u32.to_le_bytes()); // host-visible I/O min
    out.extend_from_slice(&max.to_le_bytes()); // host-visible I/O max
    out.extend_from_slice(&base.to_le_bytes()); // translation to CPU MMIO window
    out.extend_from_slice(&size.to_le_bytes());
    out
}

pub(crate) fn resource_qword_memory_with_flags(base: u64, size: u64, type_flags: u8) -> Vec<u8> {
    let max = base
        .checked_add(size)
        .and_then(|v| v.checked_sub(1))
        .expect("QWordMemory range overflow");
    let mut out = vec![0x8A, 0x2B, 0x00, 0x00, 0x0C, type_flags];
    out.extend_from_slice(&0u64.to_le_bytes()); // granularity
    out.extend_from_slice(&base.to_le_bytes());
    out.extend_from_slice(&max.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes()); // translation offset
    out.extend_from_slice(&size.to_le_bytes());
    out
}

pub(crate) fn resource_qword_memory(base: u64, size: u64) -> Vec<u8> {
    // Memory, min/max fixed, non-cacheable, read-write.
    resource_qword_memory_with_flags(base, size, 0x01)
}

pub(crate) fn resource_qword_prefetchable_memory(base: u64, size: u64) -> Vec<u8> {
    // Memory, min/max fixed, prefetchable, read-write.  In an ACPI address-space
    // descriptor bits 2:1 of the memory-specific flags encode cacheability and
    // 0b11 means prefetchable; bit 0 selects read-write.
    resource_qword_memory_with_flags(base, size, 0x07)
}

pub(crate) fn resource_end_tag() -> [u8; 2] {
    [0x79, 0x00]
}
