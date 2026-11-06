//! The microphone source with fake input queues: no test opens a device.

use super::*;

const MS: usize = BYTES_PER_MS;

/// Logs "open" when the supervisor opens a queue and "close" when it drops it.
fn logging_opener(log: &Arc<Mutex<Vec<&'static str>>>) -> OpenInput {
    struct Running(Arc<Mutex<Vec<&'static str>>>);
    impl Drop for Running {
        fn drop(&mut self) {
            self.0.lock().unwrap().push("close");
        }
    }
    let log = Arc::clone(log);
    Box::new(move |_ring| {
        log.lock().unwrap().push("open");
        Ok(Box::new(Running(Arc::clone(&log))) as RunningInput)
    })
}

fn wait_for(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(1));
    }
}

/// Frames whose four bytes are their own index, from `first`, for `ms`.
fn frames(first: u32, ms: usize) -> Vec<u8> {
    (first..first + ms as u32 * 48)
        .flat_map(u32::to_le_bytes)
        .collect()
}

fn read(source: &mut MicrophoneSource, out: &mut [u8]) -> usize {
    source.read_pcm(out, 48_000, 2, 16)
}

#[test]
fn the_input_opens_only_while_the_guest_captures_and_closes_after_the_grace() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let grace = Duration::from_millis(200);
    let mut source = MicrophoneSource::new(logging_opener(&log), grace).unwrap();
    thread::sleep(Duration::from_millis(20));
    assert!(
        log.lock().unwrap().is_empty(),
        "nothing opens before the guest captures"
    );
    source.stream_started();
    wait_for("open", || log.lock().unwrap().len() == 1);
    // A restart within the grace keeps the queue open.
    source.stream_stopped();
    thread::sleep(grace / 4);
    source.stream_started();
    thread::sleep(grace * 2);
    assert_eq!(*log.lock().unwrap(), ["open"]);
    let stopped = Instant::now();
    source.stream_stopped();
    wait_for("close", || log.lock().unwrap().len() == 2);
    assert!(stopped.elapsed() >= grace, "closed before the grace ended");
    // The next capture opens a new queue, on the then-default input; drop closes it.
    source.stream_started();
    wait_for("reopen", || log.lock().unwrap().len() == 3);
    drop(source);
    assert_eq!(*log.lock().unwrap(), ["open", "close", "open", "close"]);
}

#[test]
fn the_guest_reads_after_priming_and_its_delay_stays_bounded() {
    let open: OpenInput = Box::new(|_ring| Ok(Box::new(()) as RunningInput));
    let mut source = MicrophoneSource::new(open, STOP_GRACE).unwrap();
    source.stream_started();
    let mut out = vec![0u8; 5 * MS];
    source.ring.publish(&frames(0, 20));
    assert_eq!(
        read(&mut source, &mut out),
        0,
        "20 ms is below the prime level"
    );
    source.ring.publish(&frames(960, 20));
    assert_eq!(read(&mut source, &mut out), 5 * MS);
    assert_eq!(out, frames(0, 5));
    // The ring runs dry: silence fills the rest and the next read primes again.
    let mut long = vec![0u8; 100 * MS];
    assert_eq!(read(&mut source, &mut long), 35 * MS);
    assert_eq!(&long[..35 * MS], &frames(240, 35)[..]);
    source.ring.publish(&frames(1_920, 10));
    assert_eq!(read(&mut source, &mut out), 0);
    // 160 ms buffered: the oldest goes, down to the newest 30 ms.
    source.ring.publish(&frames(2_400, 150));
    assert_eq!(read(&mut source, &mut out), 5 * MS);
    assert_eq!(out, frames(9_600 - 30 * 48, 5));
    // Another format reads silence; a new capture stream starts from an empty ring.
    assert_eq!(source.read_pcm(&mut out, 44_100, 2, 16), 0);
    source.stream_stopped();
    source.stream_started();
    source.ring.publish(&frames(0, 20));
    assert_eq!(read(&mut source, &mut out), 0);
}

#[test]
fn an_input_that_cannot_open_prints_one_line_and_the_guest_records_silence() {
    let opens = Arc::new(Mutex::new(0));
    let counter = Arc::clone(&opens);
    let open: OpenInput = Box::new(move |_ring| {
        *counter.lock().unwrap() += 1;
        Err("AudioQueueNewInput failed with OSStatus -50 (0xffffffce)".into())
    });
    let mut source = MicrophoneSource::new(open, STOP_GRACE).unwrap();
    source.stream_started();
    wait_for("the notice", || !source.notices.lock().unwrap().is_empty());
    assert_eq!(
        *source.notices.lock().unwrap(),
        [
            "HDA capture: the Mac microphone did not start (AudioQueueNewInput failed with \
          OSStatus -50 (0xffffffce)); the guest microphone records silence"
        ]
    );
    let mut out = [0u8; 5 * MS];
    assert_eq!(read(&mut source, &mut out), 0);
    assert!(
        source.notices.lock().unwrap().is_empty(),
        "the guest read printed it"
    );
    // The same capture stream does not retry in a loop.
    thread::sleep(Duration::from_millis(50));
    assert_eq!(*opens.lock().unwrap(), 1);
}
