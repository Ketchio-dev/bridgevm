//! Invalid row metadata refuses the whole snapshot before output mutation.

use super::*;

#[test]
fn malformed_rows_at_either_end_preserve_previous_output_and_capacity() {
    let valid = "new.txt|4|0|new-stamp\n";
    for bad in [
        "missing-fields",
        "missing-mtime|1|0",
        "bad-size|nope|0|stamp",
        "missing-size||0|stamp",
        "negative-size|-1|0|stamp",
        "overflow|18446744073709551616|0|stamp",
        "bad-flag|1|2|stamp",
        "missing-flag|1||stamp",
        "padded-flag|1|01|stamp",
        "spaced-flag|1| 0|stamp",
    ] {
        for listing in [format!("{bad}\n{valid}"), format!("{valid}{bad}\n")] {
            let mut out = Vec::with_capacity(8);
            out.extend(parse_ls("old.txt|7|0|old-stamp"));
            let previous = out.clone();
            let capacity = out.capacity();
            let allocation = out.as_ptr();
            assert!(
                parse_ls_into(&listing, &mut out).is_err(),
                "accepted {bad:?}"
            );
            assert_eq!(out, previous, "partly applied {bad:?}");
            assert_eq!(out.capacity(), capacity, "reallocated for {bad:?}");
            assert_eq!(out.as_ptr(), allocation, "replaced output for {bad:?}");
        }
    }
}

#[test]
fn valid_rows_keep_right_split_pipe_names_opaque_stamps_and_directory_flags() {
    let mut out = Vec::with_capacity(4);
    out.extend(parse_ls("old.txt|1|0|old"));
    let capacity = out.capacity();
    let refused = parse_ls_into(
        "sub\\odd|name.txt|18446744073709551615|0|opaque stamp 가\r\n\r\n\
         folder|0|1|\n",
        &mut out,
    )
    .unwrap();
    assert_eq!(refused.count, 0);
    assert_eq!(out.capacity(), capacity);
    assert_eq!(
        out,
        [
            LsEntry {
                name: "sub/odd|name.txt".into(),
                size: u64::MAX,
                is_dir: false,
                mtime: "opaque stamp 가".into(),
            },
            LsEntry {
                name: "folder".into(),
                size: 0,
                is_dir: true,
                mtime: String::new(),
            },
        ]
    );
}

#[test]
fn valid_metadata_keeps_existing_name_refusals_without_refusing_good_rows() {
    let overlong_component = "n".repeat(256);
    let listing =
        format!("../escape|1|0|stamp\n{overlong_component}|1|0|stamp\nok.txt|3|0|stamp\n");
    let mut out = Vec::new();
    let refused = parse_ls_into(&listing, &mut out).unwrap();
    assert_eq!(
        refused.count, 1,
        "only the existing host-name limit is counted"
    );
    assert_eq!(
        out,
        [LsEntry {
            name: "ok.txt".into(),
            size: 3,
            is_dir: false,
            mtime: "stamp".into(),
        }]
    );
}
