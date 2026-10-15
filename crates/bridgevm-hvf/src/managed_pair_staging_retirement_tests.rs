use super::*;

#[test]
fn successful_publisher_with_old_debris_keeps_ownership_for_later_retry() {
    let fixture = Fixture::new("owned-retirement-old-debris", true, false);
    let current = fixture.root.join("current");
    fs::remove_file(current.join(PIN_NAME)).unwrap();
    let old = contents(&current);
    let parked = fixture.scratch.path("legacy-current");
    let mut pair = fixture.pair();
    let published = pair.restore_with_capacity(
        &fixture.snapshot,
        crate::snapshot_pair::free_space::available_bytes,
        |stage, destination| {
            fs::rename(destination, &parked)?;
            fs::rename(stage, destination)?;
            fs::rename(&parked, stage)?;
            fs::remove_file(stage.join("disk.raw"))?;
            fs::remove_file(stage.join("manifest.json"))?;
            // Publication may report success after best-effort old-generation
            // removal fails. The ownership retirement must preserve that fact.
            Ok(())
        },
    );
    assert_eq!(contents(&current), fixture.source);
    assert_eq!(fs::read(fixture.stage.join("vars.fd")).unwrap(), old[1]);
    drop(pair);
    let before = fixture::namespace::capture(&fixture.scratch.0);
    let mut pair = fixture.pair();
    let retried = pair.restore(&fixture.snapshot);
    assert_eq!(
        fs::read(fixture.stage.join("vars.fd")).ok(),
        Some(old[1].clone())
    );
    assert!(published.is_err());
    assert!(retried.is_err());
    assert_eq!(fixture::namespace::capture(&fixture.scratch.0), before);
    assert!(fixture.root.join(ROOT_NAME).is_file());
    assert!(published
        .unwrap_err()
        .to_string()
        .contains("it was left intact"));
    drop(pair);
    let preserved = fixture.scratch.path("preserved-legacy-current");
    fs::rename(&fixture.stage, &preserved).unwrap();
    fixture.pair().restore(&fixture.snapshot).unwrap();
    assert_eq!(fs::read(preserved.join("vars.fd")).unwrap(), old[1]);
    assert!(!fixture.root.join(ROOT_NAME).exists());
}
