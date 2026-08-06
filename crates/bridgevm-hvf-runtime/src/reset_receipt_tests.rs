use super::*;
use crate::ResetGeneration;

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("bv-receipt-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn a_receipt_proves_the_generation_it_was_written_for() {
    let dir = scratch("proves");
    let disk = dir.join("disk.raw");
    fs::write(&disk, b"payload").expect("disk");
    let generation = ResetGeneration::new();
    let tag = generation.advance();
    let receipt = dir.join("reset-receipt");
    flush_and_write_receipt(&[&disk], &receipt, tag).expect("flush");
    assert!(receipt_proves_flush(&receipt, tag));
    let body = fs::read_to_string(&receipt).expect("read");
    assert!(
        body.contains("disk.raw"),
        "receipt names what it flushed: {body}"
    );
}

#[test]
fn a_receipt_from_another_generation_proves_nothing() {
    // The stale artifact of an earlier reset must not authorize a restart.
    let dir = scratch("stale");
    let disk = dir.join("disk.raw");
    fs::write(&disk, b"payload").expect("disk");
    let generation = ResetGeneration::new();
    let old = generation.advance();
    let receipt = dir.join("reset-receipt");
    flush_and_write_receipt(&[&disk], &receipt, old).expect("flush");
    let new = generation.advance();
    assert!(!receipt_proves_flush(&receipt, new));
    assert!(
        receipt_proves_flush(&receipt, old),
        "the old tag still matches its own receipt"
    );
}

#[test]
fn a_missing_flush_target_aborts_before_any_receipt_exists() {
    // Order is the contract: if a named file cannot be flushed, the receipt
    // must not appear -- "no receipt, no restart" only works if the receipt
    // is written last.
    let dir = scratch("abort");
    let receipt = dir.join("reset-receipt");
    let missing = dir.join("nonexistent.raw");
    let tag = ResetGeneration::new().advance();
    let error = flush_and_write_receipt(&[&missing], &receipt, tag).expect_err("missing file");
    assert!(matches!(
        error,
        RuntimeError::Io {
            context: "flush storage before reset",
            ..
        }
    ));
    assert!(
        !receipt.exists(),
        "no receipt may exist after a failed flush"
    );
}

#[test]
fn an_absent_receipt_proves_nothing() {
    let dir = scratch("absent");
    let tag = ResetGeneration::new().advance();
    assert!(!receipt_proves_flush(&dir.join("never-written"), tag));
}
