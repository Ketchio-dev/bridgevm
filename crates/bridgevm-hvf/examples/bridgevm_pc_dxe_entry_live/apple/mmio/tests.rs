use super::*;

#[test]
fn decodes_unsigned_word_read_and_write() {
    let read = DataAbort::decode(0x9180_8005).unwrap();
    assert_eq!(read.size, 4);
    assert_eq!(read.register, 0);
    assert!(!read.write);
    let write = DataAbort::decode(0x9189_8045).unwrap();
    assert_eq!(write.size, 4);
    assert_eq!(write.register, 9);
    assert!(write.write);
}

#[test]
fn rejects_an_abort_without_instruction_syndrome() {
    assert!(DataAbort::decode(EC_DATA_ABORT << 26).is_err());
}
