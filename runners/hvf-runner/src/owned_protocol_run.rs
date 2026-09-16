//! Product stdin/stdout ownership boundary over the shared typed execution core.
use super::{
    owned::{execution, signals},
    PRODUCT_POLICY,
};
use anyhow::{bail, Context, Result};
use bridgevm_hvf_runtime::{prepare, LaunchManifest, RuntimeControl};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
#[path = "owned_protocol_channel.rs"]
mod channel;
#[path = "owned_protocol_codec.rs"]
mod codec;
#[path = "owned_protocol.rs"]
mod dto;
#[path = "owned_protocol_ledger.rs"]
mod ledger;
#[path = "owned_protocol_outcome.rs"]
mod outcome;
#[path = "owned_protocol_io.rs"]
mod pipe_io;
#[path = "owned_protocol_state.rs"]
mod state;
#[path = "owned_protocol_worker.rs"]
mod worker;
pub(super) fn run(spec: &str, args: &crate::Args) -> Result<()> {
    if spec == "-"
        || args.typed.helper.is_none()
        || args.typed.helper_evidence_dir.is_none()
        || args.typed.helper_vtpm_key_stdin
    {
        bail!("invalid owned runtime mode combination");
    }
    let helper = args
        .typed
        .helper
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("owned helper missing"))?;
    let _signals =
        signals::CancellationSignals::install().context("install runtime cancellation")?;
    let bytes = super::manifest_input::read_owned(spec)?;
    let digest = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let text = std::str::from_utf8(&bytes).context("launch manifest encoding")?;
    let manifest = LaunchManifest::parse(text, PRODUCT_POLICY)
        .map_err(|_| anyhow::anyhow!("launch manifest rejected"))?;
    let (mut channel, mut hello) =
        channel::Channel::open(&digest, args.typed.helper_vtpm_state.is_some())
            .context("owned runtime handshake failed")?;
    let mut state_key = codec::take_key(&mut hello);
    let ledger = RefCell::new(ledger::Ledger::default());
    let requested = || channel.requested();
    let unconfirmed = |value: bridgevm_hvf_runtime::CleanupUnconfirmed| {
        eprintln!(
            "owned cleanup unconfirmed: {:?} pid={}; retaining ownership",
            value.role, value.pid
        );
    };
    let observe = |fact| {
        let mut facts = ledger.borrow_mut();
        if let Some(event) = facts.observe(fact) {
            channel.emit(event);
        }
        if facts.invalid {
            channel.state.lock().unwrap().broken();
        }
    };
    let control = RuntimeControl::with_observer(&requested, &unconfirmed, &observe);
    let mut cause = "startupFailed";
    let mut failure = None;
    let mut admitted = false;
    if !control.is_cancelled() {
        match prepare(manifest, "hvf-runner --owned-runtime-stdio") {
            Err(error) => {
                eprintln!("launch refused: {error}");
                failure = Some("mediaAdmissionFailed");
            }
            Ok(prepared) => {
                admitted = true;
                let mut ready = dto::Event::new("ready");
                ready.ready = Some(dto::Ready {
                    manifest_sha256: digest,
                    generation: 0,
                    term_millis: 2000,
                    kill_reap_millis: 2000,
                });
                channel.emit(ready);
                let (result, cleanup) =
                    execution::execute(&prepared, args, helper, state_key.take(), &control);
                // execute cannot return with an unreaped child. Both leases remain through it.
                drop(prepared);
                match result {
                    Ok(run) => {
                        cause = if run.cycles.len() == args.supervise_max_cycles as usize
                            && (run.cycles.is_empty()
                                || ledger
                                    .borrow()
                                    .helper
                                    .last
                                    .as_ref()
                                    .is_some_and(|last| last.status == Some(42)))
                        {
                            "cycleBudget"
                        } else {
                            "normalExit"
                        };
                    }
                    Err(error) => {
                        eprintln!("runtime failed: {error}");
                        cause = "runtimeFailed";
                        failure = outcome::failure(
                            &error,
                            control.is_cancelled(),
                            ledger.borrow().helper.spawned_count > 0,
                        );
                    }
                }
                if let Err(error) = cleanup {
                    eprintln!("swtpm cleanup failed: {error}");
                    failure = Some("runtimeDirectoryCleanupFailed");
                }
            }
        }
    }
    if let Some(key) = state_key.as_mut() {
        key.fill(0);
    }
    // Observe final signals before closing stop admission. No control callback is needed after this scope.
    let cancelled = control.is_cancelled();
    let facts = ledger.into_inner();
    if !facts.complete() {
        bail!("owned lifecycle observations inconsistent; completion unproven");
    }
    if facts.directory_name() == "removeFailed" {
        failure = Some("runtimeDirectoryCleanupFailed");
    }
    let complete = dto::Complete {
        operation_id: None,
        cause: cause.into(),
        outcome: if failure.is_some() {
            "failed"
        } else if cancelled {
            "cancelled"
        } else {
            "finished"
        }
        .into(),
        failure_code: failure.map(str::to_owned),
        helper: facts.helper.clone(),
        swtpm: facts.swtpm.clone(),
        media_lease_disposition: if admitted {
            "releasedAfterReap"
        } else {
            "notAdmitted"
        }
        .into(),
        runtime_directory_disposition: facts.directory_name().into(),
    };
    if !channel.finish(complete) {
        bail!("owned runtime channel failed; completion delivery unproven");
    }
    if cancelled || failure.is_some() {
        bail!("owned runtime ended; cleanup observed; guest shutdown unproven");
    }
    Ok(())
}
