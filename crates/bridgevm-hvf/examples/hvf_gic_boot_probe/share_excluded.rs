//! Names the share never syncs either way: files and folders the Mac or Windows
//! writes for itself. The host walk skips them without a `stat`, the guest
//! listing drops them, and `host_held` keeps every fetch and deletion off them,
//! so a copy one side already has stays as it is.
//!
//! The rule reads the name alone, never what else a folder holds or what was
//! synced before, so a name is excluded on both sides, in every session and for
//! good. A rule that can change its answer turns a held name into a shared one,
//! and the side that lacks it then fetches or deletes it: the class of failure
//! behind share-interrupt s2.

use super::*;

/// In any folder:
/// - `.DS_Store`: Finder writes one in every folder it opens.
/// - `.localized`: the empty marker Finder hides in Documents and Downloads and
///   Explorer shows.
/// - `Icon\r`: a custom folder icon; Windows refuses the `\r`, so every upload
///   would fail and be logged.
/// - `Thumbs.db`, `desktop.ini`: Explorer's thumbnail cache and folder settings.
///   The agent lists hidden and system files, and these have no leading dot, so
///   Finder would show them; uploaded from the Mac, they lose their Hidden and
///   System attributes, so Explorer would show them too.
const IN_ANY_FOLDER: [&str; 5] = [".DS_Store", ".localized", "Icon\r", "Thumbs.db", "desktop.ini"];

/// At the share root only, with everything below: a volume's metadata when the
/// share is a whole drive. `.Trashes` holds trashed files and `.TemporaryItems`
/// short-lived save temporaries; `$RECYCLE.BIN` and `System Volume Information`
/// are on a drive Windows has used; the last three are volume markers. A user's
/// `sub/.fseventsd` is a folder like any other.
const AT_ROOT: [&str; 10] = [
    ".fseventsd",
    ".Spotlight-V100",
    ".Trashes",
    ".DocumentRevisions-V100",
    ".TemporaryItems",
    "$RECYCLE.BIN",
    "System Volume Information",
    ".VolumeIcon.icns",
    ".apdisk",
    ".com.apple.timemachine.donotpresent",
];

/// Whether the share key `name` (`/`-separated, as both sides key names) is OS
/// metadata or lies below such a name.
///
/// - Each part is compared as Windows and the engine compare names
///   (`windows_key`: case and trailing dots and spaces ignored). An exact match
///   would let `.ds_store` or `desktop.ini.` through, and the rename and
///   name-clash rules would then pair it with the excluded copy on the other
///   side.
/// - Every part counts, not only the last: the walk does not go below an
///   excluded folder, so the guest's files below one must be excluded as well,
///   or the Mac would fetch them, never see them, and delete them in the guest.
/// - `._` (AppleDouble, on exFAT, FAT and SMB, and in unzipped `__MACOSX`) is
///   excluded always, not only beside its `<name>`: that sibling differs between
///   the two sides and over time, so `._foo` would turn into a file to fetch the
///   moment Windows deleted `foo`. macOS deletes `._foo` with `foo`, so a lone one
///   is a leftover; a user's own `._x`, which Finder hides anyway, never syncs.
pub fn is_share_excluded(name: &str) -> bool {
    let named = |key: &str, names: &[&str]| names.iter().any(|name| key.eq_ignore_ascii_case(name));
    name.split('/').map(windows_key).enumerate().any(|(i, key)| {
        (i == 0 && named(&key, &AT_ROOT)) || key.starts_with("._") || named(&key, &IN_ANY_FOLDER)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_metadata_names_and_everything_below_them_are_excluded_and_look_alikes_are_not() {
        for excluded in [
            ".DS_Store",
            ".ds_store",
            "sub/deep/.DS_STORE",
            ".DS_Store.",
            "desktop.ini ",
            "a/.localized",
            "Icon\r",
            "sub/ICON\r",
            "Thumbs.db",
            "sub/thumbs.DB",
            "desktop.ini",
            "sub/DESKTOP.INI",
            "._a.txt",
            "sub/._A.TXT",
            "._",
            "._folder/a.txt",
            ".DS_Store/x",
            ".fseventsd",
            ".FSEVENTSD/0000000000a1b2c3",
            ".Spotlight-V100/Store-V2/store.db",
            ".trashes/501/x",
            ".DocumentRevisions-V100/x",
            ".TemporaryItems/folders.501/x",
            "$Recycle.Bin/S-1-5-21/x",
            "System Volume Information",
            "system volume information/x",
            ".VolumeIcon.icns",
            ".APDISK",
            ".com.apple.timemachine.donotpresent",
        ] {
            assert!(is_share_excluded(excluded), "{excluded:?}");
        }
        for shared in [
            "",
            "a.txt",
            ".DS_Store.txt",
            "a.DS_Store",
            "sub/.DS_Store.txt",
            ".localized.txt",
            "Icon",
            "Icon\r.png",
            "Thumbs.db.bak",
            "my desktop.ini",
            "_.a.txt",
            "a._b",
            "sub/a._b",
            ".fseventsd2",
            ".fseventsd2/x",
            "sub/.fseventsd/notes.txt",
            "sub/.Trashes/x",
            "sub/$RECYCLE.BIN/x",
            "sub/System Volume Information",
            "sub/.VolumeIcon.icns",
            "sub/.apdisk",
            ".Trashes2",
            ".bridgevm-part-notes",
        ] {
            assert!(!is_share_excluded(shared), "{shared:?}");
        }
    }

    /// Whatever records exist, neither delete loop acts on an excluded name.
    #[test]
    fn no_record_ever_deletes_an_excluded_name_on_either_side() {
        // A guest edit: not the size the PUT wrote.
        let edited = |name: &str| LsEntry { name: name.into(), size: 4, is_dir: false, mtime: "g2".into() };
        for name in [".DS_Store", "sub/._a.txt", ".fseventsd/0000a"] {
            let mut sync = ShareSync::new(512);
            sync.on_put_ok(name.into(), 3, fnv1a64(b"abc"));
            let actions = sync.on_host_scan(Vec::new());
            assert!(actions.is_empty(), "{name}: {actions:?}");
            let actions = sync.on_guest_listing(Vec::new());
            assert!(actions.is_empty(), "{name}: {actions:?}");
            // Nor is a guest copy fetched over the Mac's.
            let actions = sync.on_guest_listing([edited(name)]);
            assert!(actions.is_empty(), "{name}: {actions:?}");
        }
        // A name like any other still is deleted.
        let mut sync = ShareSync::new(512);
        sync.on_put_ok("a.txt".into(), 3, fnv1a64(b"abc"));
        assert_eq!(sync.on_host_scan(Vec::new()), [SyncAction::DeleteGuest { name: "a.txt".into() }]);
    }
}
