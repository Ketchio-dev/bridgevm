//! Both frontends share the same helper/TPM teardown and borrowed media lifetime.
use anyhow::{Context, Result};
use bridgevm_hvf_runtime::{
    run_prepared_vm, start_swtpm_controlled, ControlledRun, PreparedVm, RuntimeControl, VtpmConfig,
};
use std::path::Path;
pub(crate) fn execute(
    prepared: &PreparedVm,
    args: &crate::Args,
    helper: &Path,
    state_key: Option<Vec<u8>>,
    control: &RuntimeControl<'_>,
) -> (
    Result<ControlledRun>,
    Result<Option<std::process::ExitStatus>, bridgevm_hvf_runtime::RuntimeError>,
) {
    let mut launch = super::options::build(args, helper);
    let mut swtpm = None;
    let result = (|| -> Result<_> {
        if let Some(state_dir) = &args.typed.helper_vtpm_state {
            let mut config = VtpmConfig {
                state_dir: state_dir.clone(),
                swtpm_bin: args
                    .typed
                    .helper_swtpm_bin
                    .clone()
                    .unwrap_or_else(|| "/opt/homebrew/bin/swtpm".into()),
                state_key,
            };
            let process = start_swtpm_controlled(&config, control);
            if let Some(key) = config.state_key.as_mut() {
                key.fill(0);
            }
            let process = process
                .map_err(anyhow::Error::new)
                .context("vTPM start failed")?;
            launch.swtpm_sockets = Some((
                process.data_socket().to_path_buf(),
                process.control_socket().to_path_buf(),
            ));
            swtpm = Some(process);
        }
        if control.is_cancelled() {
            return run_prepared_vm(prepared, &launch, Path::new(""), 0, control)
                .map_err(|error| anyhow::anyhow!("VM cancelled: {error}"));
        }
        if let Some(surfaces) = &launch.surfaces {
            std::fs::create_dir_all(surfaces.evidence_dir.join("ramfb"))
                .context("create evidence dir")?;
        }
        let receipt = args
            .supervise_receipt
            .clone()
            .unwrap_or_else(super::default_receipt_path);
        run_prepared_vm(
            prepared,
            &launch,
            Path::new(&receipt),
            args.supervise_max_cycles,
            control,
        )
        .map_err(anyhow::Error::new)
        .context("vm run failed")
    })();
    let cleanup = swtpm
        .as_mut()
        .map(|process| process.shutdown(control))
        .transpose();
    // Both exact Child handles have now been reaped, on success and failure.
    drop(swtpm);
    (result, cleanup)
}
