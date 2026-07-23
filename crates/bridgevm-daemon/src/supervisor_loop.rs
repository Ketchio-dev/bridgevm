//! The reconcile/accept run loop, signal handling, and shutdown reaping of owned children.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_qemu::qmp_socket_path;
use bridgevm_storage::VmRuntimeState;
use bridgevm_storage::VmStore;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

/// Set by the SIGTERM/SIGINT handler so the supervisor loop can reap its
/// spawned QEMU/AppleVzRunner children before exiting. Without this, killing
/// `bridgevmd` (the common case: a service restart, or a test harness tearing
/// the daemon down) would leave its VM processes orphaned — still running and
/// still holding their ports (e.g. VNC :0 / TCP 5900) with no supervisor.
pub(crate) static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_shutdown_signal(_signal: libc::c_int) {
    // Async-signal-safe: only flips an atomic. The actual teardown happens in
    // the supervisor loop, which polls this flag.
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

pub(crate) fn install_shutdown_handlers() {
    // SAFETY: `handle_shutdown_signal` does nothing but an atomic store, which
    // is async-signal-safe, so installing it as a C signal handler is sound.
    unsafe {
        let handler = handle_shutdown_signal as *const () as libc::sighandler_t;
        libc::signal(libc::SIGTERM, handler);
        libc::signal(libc::SIGINT, handler);
    }
}

pub(crate) fn serve(
    store: VmStore,
    socket_path: &Path,
    reconcile_interval: Duration,
) -> Result<()> {
    let listener = bind_daemon_listener(socket_path)?;
    listener
        .set_nonblocking(true)
        .context("failed to configure daemon socket")?;
    println!("bridgevmd listening");
    install_shutdown_handlers();
    let mut state = DaemonState::new(store);
    let mut last_reconcile = Instant::now();
    let (request_sender, request_receiver) = mpsc::channel::<PendingDaemonRequest>();
    let active_clients = Arc::new(AtomicUsize::new(0));

    loop {
        if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
            println!("bridgevmd received shutdown signal; reaping supervised backends");
            state.shutdown_reap_children();
            println!("bridgevmd shutdown complete");
            return Ok(());
        }

        while let Ok(pending) = request_receiver.try_recv() {
            let response = state.handle_request(pending.request);
            let _ = pending.response_sender.send(response);
        }

        match listener.accept() {
            Ok(stream) => {
                spawn_connection_worker(
                    stream.0,
                    request_sender.clone(),
                    Arc::clone(&active_clients),
                );
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => eprintln!("bridgevmd accept failed: {error}"),
        }

        if last_reconcile.elapsed() >= reconcile_interval {
            if let Err(error) = state.reconcile_children() {
                eprintln!("bridgevmd supervisor failed: {error:#}");
            }
            last_reconcile = Instant::now();
        }
        thread::sleep(Duration::from_millis(25));
    }
}

impl DaemonState {
    /// Tear down every backend this daemon spawned — gracefully (QMP `quit` for
    /// Compatibility Mode, then `SIGTERM`/`SIGKILL`) — so no QEMU/AppleVzRunner
    /// child is orphaned when `bridgevmd` exits. The daemon has no re-adoption
    /// path (a restarted daemon does not reclaim children by pid), so a child it
    /// leaves behind is a pure leak that keeps holding its ports. Best-effort:
    /// failing to reap one backend is logged and does not block the rest, and
    /// any child that somehow survives a failed cleanup is force-killed.
    pub(crate) fn shutdown_reap_children(&mut self) {
        let names: Vec<String> = self.children.keys().cloned().collect();
        for name in names {
            if let Err(error) = self.cleanup_owned_backend(&name, true) {
                // The graceful path bailed (e.g. an unresponsive QMP socket).
                // If the child is still owned here, cleanup failed before
                // killing it, so force-kill so it cannot orphan; otherwise it
                // was already killed and only a later metadata step failed.
                if let Some(mut backend) = self.children.remove(&name) {
                    eprintln!(
                        "bridgevmd shutdown: graceful reap of '{name}' failed ({error:#}); force-killing"
                    );
                    let _ = backend.child.kill();
                    let _ = backend.child.wait();
                } else {
                    eprintln!(
                        "bridgevmd shutdown: reaped backend '{name}' but post-kill cleanup failed: {error:#}"
                    );
                }
            }
        }
    }

    pub(crate) fn reconcile_children(&mut self) -> Result<()> {
        let mut exited = Vec::new();
        let mut terminal = Vec::new();
        for (name, backend) in &mut self.children {
            if backend
                .child
                .try_wait()
                .with_context(|| format!("failed to poll backend '{name}'"))?
                .is_some()
            {
                exited.push(name.clone());
                continue;
            }

            let Ok((bundle, _)) = self.store.get_vm(name) else {
                continue;
            };

            if let Err(error) = reconcile_guest_tools_session(&self.store, name, backend) {
                eprintln!("bridgevmd guest-tools supervisor failed for '{name}': {error:#}");
            }
            if let Err(error) = drain_guest_tools_messages(&self.store, name, backend) {
                eprintln!("bridgevmd guest-tools drain failed for '{name}': {error:#}");
            }
            if let Err(error) = refresh_proxy_window_crop_artifacts(&self.store, name, backend) {
                eprintln!("bridgevmd proxy-window crop refresh failed for '{name}': {error}");
            }

            let socket_path = qmp_socket_path(&bundle);
            if !socket_path.exists() {
                continue;
            }

            if backend.qmp.is_none() {
                backend.qmp = connect_supervisor_qmp(&socket_path).ok();
            }

            let qmp_report = qmp_supervisor_report(&mut backend.qmp, &socket_path);
            if let Some(drain) = qmp_report.drain.as_ref() {
                if let Err(error) = write_qmp_supervisor_metadata(&self.store, name, drain) {
                    eprintln!("bridgevmd QMP supervisor metadata failed for '{name}': {error:#}");
                }
            }
            if qmp_report.terminal {
                terminal.push(name.clone());
            }
        }

        for name in exited {
            self.children.remove(&name);
            let _ = self.store.transition_state(&name, VmRuntimeState::Stopped);
            self.store
                .clear_runner_metadata(&name)
                .with_context(|| format!("failed to clear runner metadata for '{name}'"))?;
        }
        for name in terminal {
            if self.children.contains_key(&name) {
                self.cleanup_owned_backend(&name, false)
                    .with_context(|| format!("failed to clean up terminal backend '{name}'"))?;
            }
        }
        Ok(())
    }
}
