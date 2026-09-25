//! Release execution is bound to the verified app bundle containing this runner.
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn validate(args: &super::Args) -> Result<()> {
    if args.launch || args.repo_root.is_some() || args.supervise.is_some() {
        bail!("--launch, --repo-root, and --supervise are unavailable in release builds");
    }
    let helper = args.typed.helper.as_deref();
    let swtpm = args.typed.helper_swtpm_bin.as_deref();
    if helper.is_some_and(|path| !path.is_absolute()) {
        bail!("release helper executable path must be absolute: --helper");
    }
    if swtpm.is_some_and(|path| !path.is_absolute()) {
        bail!("release helper executable path must be absolute: --helper-swtpm-bin");
    }
    if args.typed.helper_vtpm_state.is_some() && swtpm.is_none() {
        bail!("release vTPM requires explicit packaged --helper-swtpm-bin");
    }
    let Some(helper) = helper else { return Ok(()) };
    let (app, expected_helper, expected_swtpm) = bundled_paths()?;
    require_exact(helper, &expected_helper, "--helper")?;
    if let Some(swtpm) = swtpm {
        require_exact(swtpm, &expected_swtpm, "--helper-swtpm-bin")?;
    }
    let status = Command::new("/usr/bin/codesign")
        .env_clear()
        .args(["--verify", "--deep", "--strict"])
        .arg(&app)
        .status()
        .context("verify packaged app signature")?;
    if !status.success() {
        bail!("release helper execution requires a verified packaged app signature");
    }
    Ok(())
}

fn bundled_paths() -> Result<(PathBuf, PathBuf, PathBuf)> {
    let runner = std::env::current_exe()
        .context("resolve release runner")?
        .canonicalize()
        .context("canonicalize release runner")?;
    let release = runner.parent().context("release runner has no parent")?;
    let target = release
        .parent()
        .context("release runner has no target directory")?;
    let resources = target
        .parent()
        .context("release runner has no resources directory")?;
    let contents = resources
        .parent()
        .context("release runner has no contents directory")?;
    let app = contents
        .parent()
        .context("release runner has no app directory")?;
    if runner.file_name().is_none_or(|name| name != "hvf-runner")
        || release.file_name().is_none_or(|name| name != "release")
        || target.file_name().is_none_or(|name| name != "target")
        || resources.file_name().is_none_or(|name| name != "Resources")
        || contents.file_name().is_none_or(|name| name != "Contents")
        || app.extension().is_none_or(|name| name != "app")
    {
        bail!("release helper execution requires packaged app topology");
    }
    Ok((
        app.to_path_buf(),
        release.join("examples/hvf_gic_boot_probe"),
        contents.join("Helpers/swtpm"),
    ))
}

fn require_exact(path: &Path, expected: &Path, option: &str) -> Result<()> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("canonicalize release {option} executable"))?;
    if canonical != expected {
        bail!("release {option} must use the packaged app executable");
    }
    Ok(())
}
