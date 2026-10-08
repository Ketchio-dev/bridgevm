use super::*;
use crate::media_lease::MediaLease;
use crate::snapshot_pair::{create_snapshot, snapshot_pair_tests::Scratch, DISK_NAME, VARS_NAME};
use std::fs;

#[test]
fn existing_destination_member_or_hardlink_owner_refuses_before_staging_cleanup() {
    for hardlink in [false, true] {
        let scratch = Scratch::new("member-lease-alias");
        let disk = scratch.write("disk", b"disk");
        let vars = scratch.write("vars", b"vars");
        let dest = scratch.path("snapshot");
        create_snapshot(&disk, &vars, &dest, "vm", false, 1024).unwrap();
        let staged = super::super::staging_path(&dest);
        fs::create_dir(&staged).unwrap();
        fs::write(staged.join(DISK_NAME), b"debris-kept").unwrap();
        let member = dest.join(VARS_NAME);
        let owned = if hardlink {
            let alias = scratch.path("alias");
            fs::hard_link(&member, &alias).unwrap();
            alias
        } else {
            member
        };
        let owner = MediaLease::acquire([owned.as_path()]).unwrap();
        assert_eq!(
            super::super::destination_lease::claim_staging(&dest)
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(fs::read(staged.join(DISK_NAME)).unwrap(), b"debris-kept");
        assert_eq!(fs::read(dest.join(VARS_NAME)).unwrap(), b"vars");
        drop(owner);
        let (_owner, rebuilt) = super::super::destination_lease::claim_staging(&dest).unwrap();
        assert!(fs::read_dir(rebuilt).unwrap().next().is_none());
    }
}

#[test]
fn leased_staging_member_is_not_reclaimed() {
    let scratch = Scratch::new("leased-staging-member");
    let dest = scratch.path("snapshot");
    let staged = super::super::staging_path(&dest);
    fs::create_dir(&staged).unwrap();
    let member = staged.join(DISK_NAME);
    fs::write(&member, b"retained").unwrap();
    let _owner = MediaLease::acquire([member.as_path()]).unwrap();
    assert_eq!(
        super::super::destination_lease::claim_staging(&dest)
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(fs::read(member).unwrap(), b"retained");
    assert!(!dest.exists());
}

#[test]
fn fresh_staging_member_paths_stay_owned_until_export_owner_drops() {
    let scratch = Scratch::new("fresh-staging-member");
    let dest = scratch.path("snapshot");
    let (owner, staged) = super::super::destination_lease::claim_staging(&dest).unwrap();
    for name in [DISK_NAME, VARS_NAME] {
        let member = staged.join(name);
        fs::write(&member, b"new").unwrap();
        assert_eq!(
            MediaLease::acquire([member.as_path()]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
    drop(owner);
    MediaLease::acquire([
        staged.join(DISK_NAME).as_path(),
        staged.join(VARS_NAME).as_path(),
    ])
    .unwrap();
}
