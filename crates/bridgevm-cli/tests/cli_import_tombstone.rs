#![cfg(unix)]

#[path = "support/import_tombstone.rs"]
mod fixture;
use fixture::{Format, Invocation};

#[test]
fn public_import_healthy_directory_has_consistent_registration() {
    fixture::check(Invocation::Cli, Format::Directory, false);
}

#[test]
fn storage_import_healthy_directory_has_consistent_registration() {
    fixture::check(Invocation::Storage, Format::Directory, false);
}

#[test]
fn public_import_healthy_tar_has_consistent_registration() {
    fixture::check(Invocation::Cli, Format::Tar, false);
}

#[test]
fn storage_import_healthy_tar_has_consistent_registration() {
    fixture::check(Invocation::Storage, Format::Tar, false);
}

#[test]
fn public_import_deleted_directory_has_consistent_registration() {
    fixture::check(Invocation::Cli, Format::Directory, true);
}

#[test]
fn storage_import_deleted_directory_has_consistent_registration() {
    fixture::check(Invocation::Storage, Format::Directory, true);
}

#[test]
fn public_import_deleted_tar_has_consistent_registration() {
    fixture::check(Invocation::Cli, Format::Tar, true);
}

#[test]
fn storage_import_deleted_tar_has_consistent_registration() {
    fixture::check(Invocation::Storage, Format::Tar, true);
}
