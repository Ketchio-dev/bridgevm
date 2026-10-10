use std::process::ExitCode;

pub(super) fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  \
         snapshot_pair_cli create <disk> <vars> <dest> <vm-id> <quota-bytes>\n  \
         snapshot_pair_cli verify <snapshot-dir>\n  \
         snapshot_pair_cli restore <snapshot-dir> <disk> <vars>\n  \
         snapshot_pair_cli lease <disk> <vars>\n  \
         snapshot_pair_cli digest <disk> <vars>\n  \
         snapshot_pair_cli size <disk> <vars>"
    );
    ExitCode::from(2)
}
