//! Independent concurrent imports must not share extraction or cleanup state.
use super::import_parallel_fixture::Fixture;
use std::fs;
use std::sync::{Arc, Barrier};

#[test]
fn serial_public_tar_imports_preserve_independent_contents() {
    let fixture = Fixture::new();
    for index in 0..4 {
        let target = fixture.target(0, index);
        let imported = target
            .import_vm(&fixture.archives[index].0, Some("copy"))
            .unwrap();
        fixture.verify(index, &imported);
    }
    fixture.verify_sources();
    fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn concurrent_public_tar_imports_preserve_independent_contents() {
    let fixture = Fixture::new();
    eprintln!("owned import fixture: {}", fixture.root.display());
    for round in 0..8 {
        let barrier = Arc::new(Barrier::new(4));
        let mut workers = Vec::new();
        for index in 0..4 {
            let target = fixture.target(round, index);
            let input = fixture.archives[index].0.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                (index, target.import_vm(input, Some("copy")))
            }));
        }
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        fixture.verify_sources();
        let errors = results
            .iter()
            .filter_map(|(index, result)| {
                result
                    .as_ref()
                    .err()
                    .map(|error| format!("{index}: {error}"))
            })
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "round {round}: {errors:?}");
        for (index, result) in results {
            fixture.verify(index, &result.unwrap());
        }
    }
    fs::remove_dir_all(fixture.root).unwrap();
}
