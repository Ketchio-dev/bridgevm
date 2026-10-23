use super::*;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const READ_CLOCK: [u8; 10] = [0x80, 0x01, 0, 0, 0, 10, 0, 0, 0x01, 0x81];
const SUCCESS: [u8; 10] = [0x80, 0x01, 0, 0, 0, 10, 0, 0, 0, 0];
const STOP: &str = "stop: TPM backend setup failed before the first guest instruction: ";

type Log = Arc<Mutex<Vec<&'static str>>>;

/// Plays swtpm on Unix sockets in a fresh directory. Every data command gets a
/// 10-byte success and every CMD_INIT `cmd_init_result`; both are logged in
/// arrival order until the backend closes its connections.
struct ScriptedSwtpm {
    dir: PathBuf,
    log: Log,
    peers: Vec<JoinHandle<()>>,
}

impl ScriptedSwtpm {
    fn start(cmd_init_result: Option<u32>) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("bv-gen-tpm-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = Log::default();
        let mut peers = vec![serve(&dir.join("data.sock"), &log, 10, "command", &SUCCESS)];
        if let Some(result) = cmd_init_result {
            let reply = result.to_be_bytes();
            peers.push(serve(
                &dir.join("control.sock"),
                &log,
                8,
                "CMD_INIT",
                &reply,
            ));
        }
        Self { dir, log, peers }
    }

    fn data(&self) -> PathBuf {
        self.dir.join("data.sock")
    }

    fn control(&self) -> PathBuf {
        self.dir.join("control.sock")
    }

    fn finish(self) -> Vec<&'static str> {
        for peer in self.peers {
            peer.join().unwrap();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
        let log = Arc::try_unwrap(self.log).expect("every peer has exited");
        log.into_inner().unwrap()
    }
}

fn serve(path: &Path, log: &Log, len: usize, event: &'static str, reply: &[u8]) -> JoinHandle<()> {
    let listener = UnixListener::bind(path).unwrap();
    let (log, reply) = (Arc::clone(log), reply.to_vec());
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let Ok(()) = stream.set_read_timeout(Some(Duration::from_secs(10))) else {
            return; // macOS: EINVAL once the client closed; the event log shows no request
        };
        let mut request = vec![0u8; len];
        while stream.read_exact(&mut request).is_ok() {
            log.lock().unwrap().push(event);
            stream.write_all(&reply).unwrap();
        }
    })
}

#[test]
fn a_booting_generation_power_cycles_once_before_its_first_tpm_command() {
    let swtpm = ScriptedSwtpm::start(Some(0));
    let mut backend = generation_tpm(Some(&swtpm.data()), Some(&swtpm.control()), false)
        .expect("setup succeeds")
        .expect("a data socket gives a backend");

    assert_eq!(backend.execute(0, &READ_CLOCK).unwrap(), SUCCESS);
    drop(backend);
    assert_eq!(swtpm.finish(), ["CMD_INIT", "command"]);
}

#[test]
fn without_a_control_socket_no_power_cycle_is_sent() {
    let swtpm = ScriptedSwtpm::start(None);
    let mut backend = generation_tpm(Some(&swtpm.data()), None, false)
        .expect("setup succeeds")
        .expect("a data socket gives a backend");

    assert_eq!(backend.execute(0, &READ_CLOCK).unwrap(), SUCCESS);
    drop(backend);
    assert_eq!(swtpm.finish(), ["command"]);
}

#[test]
fn a_checkpoint_restore_connects_without_a_power_cycle() {
    let swtpm = ScriptedSwtpm::start(Some(0));
    let mut backend = generation_tpm(Some(&swtpm.data()), Some(&swtpm.control()), true)
        .expect("setup succeeds")
        .expect("a data socket gives a backend");

    assert_eq!(backend.execute(0, &READ_CLOCK).unwrap(), SUCCESS);
    drop(backend);
    assert_eq!(swtpm.finish(), ["command"]);
}

#[test]
fn a_rejected_power_cycle_returns_the_stop_line_before_any_tpm_command() {
    let swtpm = ScriptedSwtpm::start(Some(1));

    let stop = match generation_tpm(Some(&swtpm.data()), Some(&swtpm.control()), false) {
        Err(stop) => stop,
        Ok(_) => panic!("a rejected CMD_INIT must stop the helper"),
    };

    // media_lease::prepare() prints this line and run() returns FAILURE
    // before create_vm(), so no vCPU exists to run.
    assert_eq!(
        stop,
        format!("{STOP}swtpm CMD_INIT returned control result 0x00000001")
    );
    assert_eq!(swtpm.finish(), ["CMD_INIT"]);
}

#[test]
fn without_a_data_socket_there_is_no_tpm() {
    assert!(matches!(generation_tpm(None, None, false), Ok(None)));
}
