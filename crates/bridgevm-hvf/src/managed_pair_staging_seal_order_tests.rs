use super::*;

#[test]
fn unsupported_link_preserves_appledouble_without_admitting_recovery() {
    let fixture = Fixture::new("owned-seal-unsupported-sidecars");
    let names = ["._disk.raw", "._vars.fd", "._manifest.json"];
    for name in names {
        fs::write(fixture.stage.join(name), b"synthetic filesystem metadata").unwrap();
    }
    let mut linked = false;
    let mut synced = Vec::new();
    let result = create_using(
        &fixture.root,
        &fixture.stage,
        fixture.identity,
        &fixture.manifest,
        |source, target| {
            linked = true;
            assert_eq!(
                fs::metadata(source)?.len(),
                0,
                "the unsupported probe precedes record encoding"
            );
            assert!(!target.exists());
            Err(io::Error::from_raw_os_error(libc::ENOTSUP))
        },
        |directory| {
            synced.push(directory.to_path_buf());
            sync(directory)
        },
    )
    .unwrap();
    assert!(result.is_none());
    assert!(linked);
    assert_eq!(synced, std::slice::from_ref(&fixture.root));
    assert!(!fixture.root.join(ROOT_NAME).exists());
    assert!(!fixture.stage.join(PIN_NAME).exists());
    for name in names {
        assert_eq!(
            fs::read(fixture.stage.join(name)).unwrap(),
            b"synthetic filesystem metadata"
        );
    }
    fixture.assert_media();
}

#[test]
fn supported_link_still_refuses_extra_entries_and_preserves_them() {
    for name in ["._disk.raw", "keep.txt"] {
        let fixture = Fixture::new(&format!("owned-seal-supported-extra-{name}"));
        fs::write(fixture.stage.join(name), b"preserved extra content").unwrap();
        let mut linked = false;
        let result = create_using(
            &fixture.root,
            &fixture.stage,
            fixture.identity,
            &fixture.manifest,
            |source, target| {
                fs::hard_link(source, target)?;
                linked = true;
                Ok(())
            },
            sync,
        );
        assert!(linked, "test did not exercise a supported actual link");
        assert!(result.is_err());
        assert_eq!(
            fs::read(fixture.stage.join(name)).unwrap(),
            b"preserved extra content"
        );
        assert!(!fixture.root.join(ROOT_NAME).exists());
        assert!(!fixture.stage.join(PIN_NAME).exists());
        fixture.assert_media();
    }
}

#[test]
fn replaced_stage_is_refused_before_link_without_pin_pollution() {
    let fixture = Fixture::new("owned-seal-prelink-replaced-stage");
    let original = fixture.scratch.path("original-stage");
    fs::rename(&fixture.stage, &original).unwrap();
    private_directory(&fixture.stage, true).unwrap();
    fs::write(fixture.stage.join("keep.txt"), b"foreign replacement stage").unwrap();
    let result = create_using(
        &fixture.root,
        &fixture.stage,
        fixture.identity,
        &fixture.manifest,
        |_, _| panic!("a replaced stage reached the link probe"),
        sync,
    );
    assert!(result.is_err());
    assert!(!fixture.stage.join(PIN_NAME).exists());
    assert!(!original.join(PIN_NAME).exists());
    assert_eq!(
        fs::read(fixture.stage.join("keep.txt")).unwrap(),
        b"foreign replacement stage"
    );
    assert_eq!(
        crate::snapshot_pair::verify_snapshot(&original).unwrap(),
        fixture.manifest
    );
}
