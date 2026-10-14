use super::*;

#[test]
fn partial_exchanged_legacy_generation_is_preserved_across_retries() {
    let fixture = Fixture::new("owned-legacy-exchange", true, true);
    let current = fixture.root.join("current");
    // A generation created before ownership seals had no retained pin.
    fs::remove_file(current.join(PIN_NAME)).unwrap();
    let parked = fixture.scratch.path("legacy-current");
    let old = contents(&current);
    fs::rename(&current, &parked).unwrap();
    fs::rename(&fixture.stage, &current).unwrap();
    fs::rename(&parked, &fixture.stage).unwrap();
    // Model the existing publication cleanup interrupted after these members.
    fs::remove_file(fixture.stage.join("disk.raw")).unwrap();
    fs::remove_file(fixture.stage.join("manifest.json")).unwrap();
    let before = fixture::namespace::capture(&fixture.scratch.0);
    for _ in 0..2 {
        let mut pair = fixture.pair();
        let result = pair.restore(&fixture.snapshot);
        assert_eq!(
            fs::read(fixture.stage.join("vars.fd")).ok(),
            Some(old[1].clone())
        );
        assert!(result.is_err());
        assert_eq!(fixture::namespace::capture(&fixture.scratch.0), before);
        assert_eq!(contents(&current), fixture.source);
        assert_eq!(fs::read(fixture.stage.join("vars.fd")).unwrap(), old[1]);
    }
    let preserved = fixture.scratch.path("preserved-legacy-current");
    fs::rename(&fixture.stage, &preserved).unwrap();
    fixture.pair().restore(&fixture.snapshot).unwrap();
    assert_eq!(fs::read(preserved.join("vars.fd")).unwrap(), old[1]);
    assert!(!fixture.root.join(ROOT_NAME).exists());
}
