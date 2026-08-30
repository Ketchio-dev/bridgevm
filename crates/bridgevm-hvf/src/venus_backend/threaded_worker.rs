// Renderer-worker receive loop and idle Venus feedback polling.

const VENUS_IDLE_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1);

enum VenusWorkerWake {
    Message(VenusWorkerMessage),
    Idle,
    Disconnected,
}

fn wait_for_venus_worker(
    receiver: &std::sync::mpsc::Receiver<VenusWorkerMessage>,
) -> VenusWorkerWake {
    match receiver.recv_timeout(VENUS_IDLE_POLL_INTERVAL) {
        Ok(message) => VenusWorkerWake::Message(message),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => VenusWorkerWake::Idle,
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => VenusWorkerWake::Disconnected,
    }
}

fn run_venus_worker(
    protocol: VirtioGpuRendererProtocol,
    receiver: std::sync::mpsc::Receiver<VenusWorkerMessage>,
    init_sender: std::sync::mpsc::SyncSender<Result<(), String>>,
    fence_tx: std::sync::mpsc::Sender<CompletedFence>,
) {
    let mut backend = match VenusBackend::new_for_protocol(protocol) {
        Ok(backend) => {
            let _ = init_sender.send(Ok(()));
            backend
        }
        Err(error) => {
            let _ = init_sender.send(Err(error));
            return;
        }
    };
    loop {
        match wait_for_venus_worker(&receiver) {
            VenusWorkerWake::Message(VenusWorkerMessage::Run(job)) => job(&mut backend),
            VenusWorkerWake::Message(VenusWorkerMessage::Shutdown) => {
                // Shutdown quiet is not a stall.
                backend.poll_watchdog.disarm();
                backend.reset();
                break;
            }
            VenusWorkerWake::Idle if !backend.contexts.is_empty() => {
                // Venus feedback slots live in guest-visible shared memory and
                // do not have a virtqueue fence to trigger another VM exit.
                // Poll on the renderer-owning thread so those waits progress.
                backend.poll_fences();
                for fence in backend.drain_completed_fences() {
                    let _ = fence_tx.send(fence);
                }
            }
            VenusWorkerWake::Idle => {}
            VenusWorkerWake::Disconnected => break,
        }
    }
}

#[cfg(test)]
mod threaded_worker_tests {
    use super::*;

    #[test]
    fn idle_worker_wakes_without_a_message() {
        let (_sender, receiver) = std::sync::mpsc::channel();
        assert!(matches!(
            wait_for_venus_worker(&receiver),
            VenusWorkerWake::Idle
        ));
    }

    #[test]
    fn queued_shutdown_wins_over_idle_polling() {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send(VenusWorkerMessage::Shutdown).unwrap();
        assert!(matches!(
            wait_for_venus_worker(&receiver),
            VenusWorkerWake::Message(VenusWorkerMessage::Shutdown)
        ));
    }
}
