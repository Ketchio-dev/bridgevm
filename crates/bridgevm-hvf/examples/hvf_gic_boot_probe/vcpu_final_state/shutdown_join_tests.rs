use super::*;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};

const WAIT: Duration = Duration::from_secs(3);

struct Completion(Option<Sender<()>>);
impl Drop for Completion {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

// Release even if fixture setup/assertion panics. Both owner waits are bounded;
// the old production loop discards its later handle, so also await its final
// body acknowledgement before joining the supervisor and leaving this fixture.
struct Cleanup {
    release: Option<Sender<()>>,
    finished: Option<Receiver<()>>,
    supervisor: Option<JoinHandle<()>>,
}
impl Cleanup {
    fn finish(&mut self) -> (bool, bool) {
        if let Some(sender) = self.release.take() {
            let _ = sender.send(());
        }
        let owner_finished = self
            .finished
            .take()
            .is_some_and(|receiver| receiver.recv_timeout(WAIT * 3).is_ok());
        let supervisor_joined = self
            .supervisor
            .take()
            .is_some_and(|handle| handle.join().is_ok());
        (owner_finished, supervisor_joined)
    }
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn assert_all_owners_joined_before_outcome(leading_panics: &[bool]) {
    let shutdown = Arc::new(AtomicBool::new(false));
    let controls: Vec<_> = (1..=leading_panics.len() + 1)
        .map(|index| Arc::new(VcpuControl::new(index as u64)))
        .collect();
    let mut handles: Vec<_> = leading_panics
        .iter()
        .copied()
        .map(|must_panic| thread::spawn(move || assert!(!must_panic, "synthetic owner panic")))
        .collect();
    let (parked_tx, parked_rx) = mpsc::channel();
    let (cleanup_tx, cleanup_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();
    let held_control = Arc::clone(controls.last().unwrap());
    let owner_shutdown = Arc::clone(&shutdown);
    handles.push(thread::spawn(move || {
        let _finished = Completion(Some(finished_tx));
        let state = held_control.state.lock().unwrap();
        let _ = parked_tx.send(());
        let (state, _) = held_control
            .condvar
            .wait_timeout_while(state, WAIT, |_| !owner_shutdown.load(Ordering::SeqCst))
            .unwrap();
        drop(state);
        let _ = cleanup_tx.send(owner_shutdown.load(Ordering::SeqCst));
        let _ = release_rx.recv_timeout(WAIT);
    }));
    let set = SecondaryVcpuSet {
        shutdown,
        terminal: Arc::new(SecondaryTerminalSignal::new()),
        controls,
        handles,
    };
    let (outcome_tx, outcome_rx) = mpsc::channel();
    let supervisor = thread::spawn(move || {
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| set.shutdown_and_join()));
        let message = outcome.err().map(|error| {
            error
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_else(|| "non-string panic".into())
        });
        let _ = outcome_tx.send(message);
    });
    let mut cleanup = Cleanup {
        release: Some(release_tx),
        finished: Some(finished_rx),
        supervisor: Some(supervisor),
    };
    let parked = parked_rx.recv_timeout(WAIT);
    let saw_shutdown = cleanup_rx.recv_timeout(WAIT);
    let early_outcome = outcome_rx.recv_timeout(Duration::from_millis(300));
    let (owner_finished, supervisor_joined) = cleanup.finish();
    let returned_early = early_outcome.is_ok();
    let outcome = match early_outcome {
        Ok(value) => value,
        Err(RecvTimeoutError::Timeout) => outcome_rx.recv_timeout(WAIT).unwrap(),
        Err(error) => panic!("supervisor outcome disconnected: {error}"),
    };
    assert!(parked.is_ok() && saw_shutdown == Ok(true));
    assert!(owner_finished && supervisor_joined);
    assert_eq!(outcome.is_some(), leading_panics.contains(&true));
    if let Some(message) = outcome {
        assert!(
            message.starts_with("join secondary vCPU thread"),
            "{message}"
        );
    }
    assert!(
        !returned_early,
        "shutdown returned while a later owner was held"
    );
}

#[test]
fn first_panicked_owner_does_not_detach_later_owner() {
    assert_all_owners_joined_before_outcome(&[true]);
}

#[test]
fn later_panicked_owner_does_not_detach_remaining_owner() {
    assert_all_owners_joined_before_outcome(&[false, true]);
}

#[test]
fn multiple_panicked_owners_are_all_joined_before_fatal_outcome() {
    assert_all_owners_joined_before_outcome(&[true, true]);
}

#[test]
fn healthy_owners_remain_nonfatal_and_all_joined() {
    assert_all_owners_joined_before_outcome(&[false, false]);
}
