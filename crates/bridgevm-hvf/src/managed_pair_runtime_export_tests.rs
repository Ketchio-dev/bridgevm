use super::*;
#[path = "managed_pair_runtime_export_lifetime_tests.rs"]
mod lifetime;
use crate::snapshot_pair::snapshot_pair_tests::Scratch;

fn fixture(tag: &str) -> (Scratch, VirtBootMediaConfig, RuntimeLease) {
    let s = Scratch::new(tag);
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.flash_vars = WritableMedia::new(s.write("vars", b"vars")).with_write_back(true);
    media.nvme_disk = Some(
        WritableMedia::new(s.write("disk", b"disk"))
            .with_snapshot_path(Some(s.write("output", b"old-output"))),
    );
    let owner = acquire(&mut media).unwrap();
    (s, media, owner)
}

fn owned(path: &Path) {
    assert_eq!(
        MediaLease::acquire([path]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[test]
fn output_contention_and_absent_policy_do_not_invoke_writer() {
    let (s, _, mut owner) = fixture("stream-contention");
    let output = s.path("output");
    let competitor = MediaLease::acquire([output.as_path()]).unwrap();
    let error = owner
        .export_snapshot(RuntimeMediaSlot::Primary, |_| panic!("must refuse first"))
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(fs::read(output).unwrap(), b"old-output");
    assert!(owner
        .export_snapshot(RuntimeMediaSlot::Vars, |_| panic!("no snapshot"))
        .unwrap()
        .is_none());
    assert_eq!(
        owner
            .export_snapshot(RuntimeMediaSlot::Target, |_| panic!("absent slot"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    drop(competitor);
}

#[test]
fn writer_failure_preserves_output_and_removes_own_staging() {
    let (s, _, mut owner) = fixture("stream-writer-error");
    let before = fs::read_dir(&s.0).unwrap().count();
    let error = owner
        .export_snapshot(RuntimeMediaSlot::Primary, |out| {
            out.write_all(b"partial")?;
            Err(io::Error::other("injected writer failure"))
        })
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert_eq!(fs::read(s.path("output")).unwrap(), b"old-output");
    assert_eq!(fs::read_dir(&s.0).unwrap().count(), before);
    owned(&s.path("output"));
    owner
        .export_snapshot(RuntimeMediaSlot::Primary, |out| {
            out.write_all(b"retry")?;
            Ok(5)
        })
        .unwrap();
    assert_eq!(fs::read(s.path("output")).unwrap(), b"retry");
}

#[test]
fn streaming_publication_errors_keep_uncertain_image_owned() {
    for after_rename in [false, true] {
        let (s, _, mut owner) = fixture("stream-publish-error");
        let output = s.path("output");
        let alias = s.path("new-alias");
        let result = owner.export_snapshot_using(
            RuntimeMediaSlot::Primary,
            |out| {
                out.write_all(b"next-image")?;
                Ok(10)
            },
            |staged, destination| {
                fs::hard_link(staged, &alias)?;
                owned(staged);
                owned(destination);
                owned(&alias);
                if after_rename {
                    fs::rename(staged, destination)?;
                }
                Err(io::Error::other("injected publication sync error"))
            },
        );
        assert!(result.is_err());
        assert_eq!(
            fs::read(&output).unwrap(),
            if after_rename {
                b"next-image".as_slice()
            } else {
                b"old-output"
            }
        );
        owned(&alias);
        owned(&output);
        owner
            .persist(RuntimeMediaSlot::Vars, b"saved-vars")
            .unwrap();
        owned(&output);
        if after_rename {
            owned(&alias);
        }
        owner
            .export_snapshot(RuntimeMediaSlot::Primary, |out| {
                out.write_all(b"retry")?;
                Ok(5)
            })
            .unwrap();
        MediaLease::acquire([alias.as_path()]).unwrap();
        owned(&output);
        drop(owner);
        MediaLease::acquire([output.as_path()]).unwrap();
    }
}
