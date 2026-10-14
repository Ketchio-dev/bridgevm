#![cfg(unix)]

#[path = "support/export_preservation.rs"]
mod fixture;
use fixture::{Destination, Invocation};

#[test]
fn public_export_preserves_dangling_tar_destination() {
    fixture::check(Invocation::Cli, Destination::DanglingTar);
}

#[test]
fn storage_export_preserves_dangling_tar_destination() {
    fixture::check(Invocation::Storage, Destination::DanglingTar);
}

#[test]
fn public_export_preserves_dangling_directory_destination() {
    fixture::check(Invocation::Cli, Destination::DanglingDirectory);
}

#[test]
fn storage_export_preserves_dangling_directory_destination() {
    fixture::check(Invocation::Storage, Destination::DanglingDirectory);
}

#[test]
fn public_export_preserves_existing_tar_destination() {
    fixture::check(Invocation::Cli, Destination::ExistingTar);
}

#[test]
fn storage_export_preserves_existing_tar_destination() {
    fixture::check(Invocation::Storage, Destination::ExistingTar);
}

#[test]
fn public_export_preserves_absent_tar_roundtrip() {
    fixture::check(Invocation::Cli, Destination::AbsentTar);
}

#[test]
fn storage_export_preserves_absent_tar_roundtrip() {
    fixture::check(Invocation::Storage, Destination::AbsentTar);
}
