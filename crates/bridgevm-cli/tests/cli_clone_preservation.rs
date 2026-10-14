#![cfg(unix)]

#[path = "support/clone_preservation.rs"]
mod fixture;
use fixture::{Destination, Invocation};

#[test]
fn public_clone_preserves_dangling_destination_link() {
    fixture::check(Invocation::Cli, Destination::DanglingLink);
}

#[test]
fn storage_clone_preserves_dangling_destination_link() {
    fixture::check(Invocation::Storage, Destination::DanglingLink);
}

#[test]
fn public_clone_preserves_existing_destination() {
    fixture::check(Invocation::Cli, Destination::ExistingDirectory);
}

#[test]
fn storage_clone_preserves_existing_destination() {
    fixture::check(Invocation::Storage, Destination::ExistingDirectory);
}

#[test]
fn public_clone_copies_into_absent_destination() {
    fixture::check(Invocation::Cli, Destination::Absent);
}

#[test]
fn storage_clone_copies_into_absent_destination() {
    fixture::check(Invocation::Storage, Destination::Absent);
}
