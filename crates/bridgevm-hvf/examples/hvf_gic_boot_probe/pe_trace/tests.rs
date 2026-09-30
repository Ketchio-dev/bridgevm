use super::*;

const INJECTED_STOP: &str = "stop: PSCI 0x84000008 (system off)";

struct TestMem {
    base: u64,
    bytes: Vec<u8>,
}

impl GuestMemoryMut for TestMem {
    fn write_bytes(&mut self, _gpa: u64, _data: &[u8]) -> bool {
        false
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let off = usize::try_from(gpa.checked_sub(self.base)?).ok()?;
        self.bytes
            .get(off..off.checked_add(len)?)
            .map(<[u8]>::to_vec)
    }
}

/// A PE32+ image at RAM_BASE whose CodeView RSDS record names `pdb`.
fn image_with_pdb(pdb: &[u8]) -> TestMem {
    let mut bytes = vec![0u8; 0x2000];
    let mut put = |off: usize, data: &[u8]| bytes[off..off + data.len()].copy_from_slice(data);
    let optional = 0x98;
    let record = [b"RSDS".as_slice(), &[0; 20], pdb, &[0]].concat();
    put(0, b"MZ");
    put(0x3c, &0x80u32.to_le_bytes()); // e_lfanew
    put(0x80, b"PE\0\0");
    put(0x84, &0xaa64u16.to_le_bytes());
    put(0x94, &0xf0u16.to_le_bytes()); // SizeOfOptionalHeader
    put(optional, &0x20bu16.to_le_bytes()); // PE32+
    put(optional + 0x10, &0x1000u32.to_le_bytes()); // AddressOfEntryPoint
    put(optional + 0x18, &0x1_4000_0000u64.to_le_bytes()); // ImageBase
    put(optional + 0x38, &0x2000u32.to_le_bytes()); // SizeOfImage
    put(optional + 0x6c, &16u32.to_le_bytes()); // NumberOfRvaAndSizes
    put(optional + 0xa0, &0x400u32.to_le_bytes()); // debug directory
    put(optional + 0xa4, &28u32.to_le_bytes());
    put(0x400 + 12, &2u32.to_le_bytes()); // IMAGE_DEBUG_TYPE_CODEVIEW
    put(
        0x400 + 16,
        &u32::try_from(record.len()).unwrap().to_le_bytes(),
    );
    put(0x400 + 20, &0x500u32.to_le_bytes());
    put(0x500, &record);
    TestMem {
        base: machine::RAM_BASE,
        bytes,
    }
}

#[test]
fn frame_chain_image_summary_keeps_guest_pdb_on_one_line() {
    let mem = image_with_pdb(format!("evil.pdb\n{INJECTED_STOP}\n").as_bytes());

    let summary = pe_owner_summary(&mem, machine::RAM_BASE + 0x1000);

    assert!(!summary.contains(['\n', '\r']), "{summary:?}");
    assert_eq!(
        summary,
        format!("base=0x40000000 rva=0x1000 entry=0x1000 pdb=evil.pdb\\x0a{INJECTED_STOP}\\x0a")
    );
}

#[test]
fn image_owner_pdb_escapes_every_byte_outside_printable_ascii() {
    let mem = image_with_pdb(b"a\rb\x1b[2J\tc\xe2\x80\xa8d\x7f.pdb");

    let owner = find_pe_owner(&mem, machine::RAM_BASE + 4, 0x1000).expect("PE owner");

    assert_eq!(
        owner.pdb_path.as_deref(),
        Some("a\\x0db\\x1b[2J\\x09c\\xe2\\x80\\xa8d\\x7f.pdb")
    );
}

#[test]
fn image_record_keeps_guest_pdb_on_one_line() {
    let mem = image_with_pdb(format!("evil.pdb\r\n{INJECTED_STOP}\n").as_bytes());

    let line = pe_owner_line(&mem, "pc", machine::RAM_BASE + 0x1000);

    assert_eq!(
        line,
        format!(
            "IMAGE[pc]: addr=0x40001000 base=0x40000000 size=0x2000 rva=0x1000 entry=0x1000 \
             machine=0xaa64 preferred_base=0x140000000 pdb=evil.pdb\\x0d\\x0a{INJECTED_STOP}\\x0a"
        )
    );
}
