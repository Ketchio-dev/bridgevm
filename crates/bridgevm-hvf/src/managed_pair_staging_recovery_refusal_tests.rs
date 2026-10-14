use super::*;

#[path = "managed_pair_staging_recovery_mutations.rs"]
mod mutations;

#[test]
fn altered_ownership_or_members_preserve_selected_source_and_all_evidence() {
    for selected in [false, true] {
        for scenario in mutations::SCENARIOS {
            let fixture = Fixture::new(
                &format!("owned-refuse-{selected}-{scenario}"),
                selected,
                true,
            );
            mutations::apply(&fixture, scenario);
            fixture.assert_refused();
        }
    }
}

#[test]
fn exchanged_old_generation_never_inherits_the_new_stage_record() {
    let fixture = Fixture::new("owned-refuse-exchanged-old", true, true);
    let current = fixture.root.join("current");
    let parked = fixture.scratch.path("parked-current");
    let old = contents(&current);
    // The three renames model the completed exchange before its parent-sync
    // boundary; both original directory identities are retained.
    fs::rename(&current, &parked).unwrap();
    fs::rename(&fixture.stage, &current).unwrap();
    fs::rename(&parked, &fixture.stage).unwrap();
    for partial in [false, true] {
        if partial {
            fs::remove_file(fixture.stage.join("disk.raw")).unwrap();
        }
        let mut pair = fixture.pair();
        assert!(pair.restore(&fixture.snapshot).is_err());
        assert_eq!(contents(&current), fixture.source);
        assert_eq!(contents(&fixture.snapshot), fixture.source);
        assert_eq!(fs::read(fixture.stage.join("vars.fd")).unwrap(), old[1]);
        assert_eq!(
            fs::read(fixture.stage.join("manifest.json")).unwrap(),
            old[2]
        );
        if partial {
            assert!(!fixture.stage.join("disk.raw").exists());
        } else {
            assert_eq!(fs::read(fixture.stage.join("disk.raw")).unwrap(), old[0]);
        }
    }
}
