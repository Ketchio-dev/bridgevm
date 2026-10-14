use super::*;

#[path = "managed_pair_staging_seal_test_fixture.rs"]
mod fixture;
use fixture::Fixture;

#[test]
fn seal_syncs_record_and_both_directories_before_returning() {
    let fixture = Fixture::new("owned-seal-sync");
    let mut synced = Vec::new();
    let record = create_using(
        &fixture.root,
        &fixture.stage,
        fixture.identity,
        &fixture.manifest,
        |source, target| fs::hard_link(source, target),
        |directory| {
            let record = Record::read_pinned(&fixture.root, &fixture.stage)?;
            record.matches_stage(&fixture.stage)?;
            synced.push(directory.to_path_buf());
            sync(directory)
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(synced, [fixture.stage.clone(), fixture.root.clone()]);
    assert_eq!(record.expected_manifest().unwrap(), fixture.manifest);
    fixture.assert_media();
}

#[test]
fn only_precise_unsupported_links_allow_an_unsealed_restore() {
    for (index, code) in [
        libc::ENOTSUP,
        libc::EOPNOTSUPP,
        libc::EIO,
        libc::EPERM,
        libc::EXDEV,
        libc::EEXIST,
    ]
    .into_iter()
    .enumerate()
    {
        let fixture = Fixture::new(&format!("owned-seal-link-{index}"));
        let result = create_using(
            &fixture.root,
            &fixture.stage,
            fixture.identity,
            &fixture.manifest,
            |_, _| Err(io::Error::from_raw_os_error(code)),
            sync,
        );
        if code == libc::ENOTSUP || code == libc::EOPNOTSUPP {
            assert!(result.unwrap().is_none());
        } else {
            assert_eq!(result.unwrap_err().raw_os_error(), Some(code));
        }
        assert!(!fixture.root.join(ROOT_NAME).exists());
        assert!(!fixture.stage.join(PIN_NAME).exists());
        fixture.assert_media();
    }
    let fixture = Fixture::new("owned-seal-kind-only");
    assert!(create_using(
        &fixture.root,
        &fixture.stage,
        fixture.identity,
        &fixture.manifest,
        |_, _| Err(io::Error::new(io::ErrorKind::Unsupported, "no errno")),
        sync
    )
    .is_err());
}

#[test]
fn unsupported_link_with_a_pin_never_falls_back() {
    let fixture = Fixture::new("owned-seal-unsupported-pin");
    let result = create_using(
        &fixture.root,
        &fixture.stage,
        fixture.identity,
        &fixture.manifest,
        |source, target| {
            fs::hard_link(source, target)?;
            Err(io::Error::from_raw_os_error(libc::ENOTSUP))
        },
        sync,
    );
    assert!(result.is_err());
    fixture.assert_media();
}

#[test]
fn directory_sync_failure_aborts_and_only_cleans_owned_metadata() {
    for failed_sync in [0, 1] {
        let fixture = Fixture::new(&format!("owned-seal-sync-error-{failed_sync}"));
        let mut ordinal = 0;
        assert!(create_using(
            &fixture.root,
            &fixture.stage,
            fixture.identity,
            &fixture.manifest,
            |source, target| fs::hard_link(source, target),
            |directory| {
                let current = ordinal;
                ordinal += 1;
                if current == failed_sync {
                    Err(io::Error::other("injected directory sync"))
                } else {
                    sync(directory)
                }
            }
        )
        .is_err());
        assert!(!fixture.root.join(ROOT_NAME).exists());
        assert!(!fixture.stage.join(PIN_NAME).exists());
        fixture.assert_media();
    }
}

#[test]
fn sealing_failure_preserves_replacement_metadata() {
    let fixture = Fixture::new("owned-seal-replaced-record");
    let retained = fixture.scratch.path("original-record");
    let result = create_using(
        &fixture.root,
        &fixture.stage,
        fixture.identity,
        &fixture.manifest,
        |source, _| {
            fs::rename(source, &retained)?;
            fs::write(source, b"replacement ownership evidence")?;
            Err(io::Error::from_raw_os_error(libc::EIO))
        },
        sync,
    );
    assert_eq!(result.unwrap_err().raw_os_error(), Some(libc::EIO));
    assert_eq!(
        fs::read(fixture.root.join(ROOT_NAME)).unwrap(),
        b"replacement ownership evidence"
    );
    assert!(retained.is_file());
    fixture.assert_media();
}

#[path = "managed_pair_staging_record_tests.rs"]
mod record_tests;

#[path = "managed_pair_staging_seal_order_tests.rs"]
mod order_tests;
