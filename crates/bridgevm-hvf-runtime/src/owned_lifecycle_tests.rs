use crate::{owned_control::test_support::Fixture, *};
use std::cell::RefCell;
#[test]
fn failed_helper_still_produces_matching_actual_reap_fact() {
    let fixture = Fixture::new();
    let prepared = prepare(fixture.manifest(), "observer test").unwrap();
    let launch = fixture.launch(fixture.script("fail.sh", "exit 17"));
    let facts = RefCell::new(Vec::new());
    let observe = |event| facts.borrow_mut().push(event);
    let control = RuntimeControl::with_observer(&|| false, &|_| panic!("uncertain"), &observe);
    assert!(run_prepared_vm(
        &prepared,
        &launch,
        &fixture.root.join("receipt"),
        3,
        &control
    )
    .is_err());
    let facts = facts.borrow();
    assert_eq!(facts.len(), 2);
    let RuntimeLifecycleEvent::ChildStarted {
        role: ChildRole::Helper,
        pid,
        generation: Some(0),
    } = facts[0]
    else {
        panic!("start")
    };
    let RuntimeLifecycleEvent::ChildReaped {
        role: ChildRole::Helper,
        pid: reaped,
        generation: Some(0),
        status,
    } = facts[1]
    else {
        panic!("reap")
    };
    assert_eq!(pid, reaped);
    assert_eq!(status.code(), Some(17));
    assert!(prepare(fixture.manifest(), "still owned").is_err());
}
#[test]
fn failed_tpm_readiness_reports_spawn_reap_and_directory_removal() {
    let fixture = Fixture::new();
    let facts = RefCell::new(Vec::new());
    let observe = |event| facts.borrow_mut().push(event);
    let control = RuntimeControl::with_observer(&|| false, &|_| panic!("uncertain"), &observe);
    assert!(start_swtpm_controlled(
        &VtpmConfig {
            state_dir: fixture.root.join("state"),
            swtpm_bin: fixture.script("tpm-fail.sh", "exit 19"),
            state_key: None
        },
        &control
    )
    .is_err());
    let facts = facts.borrow();
    assert!(matches!(
        facts[0],
        RuntimeLifecycleEvent::SwtpmDirectory(DirectoryDisposition::Created)
    ));
    assert!(matches!(
        facts[1],
        RuntimeLifecycleEvent::ChildStarted {
            role: ChildRole::Swtpm,
            generation: None,
            ..
        }
    ));
    assert!(
        matches!(facts[2],RuntimeLifecycleEvent::ChildReaped {role:ChildRole::Swtpm,generation:None,status,..} if status.code()==Some(19))
    );
    assert!(matches!(
        facts[3],
        RuntimeLifecycleEvent::SwtpmDirectory(DirectoryDisposition::Removed)
    ));
    assert_eq!(facts.len(), 4);
}
