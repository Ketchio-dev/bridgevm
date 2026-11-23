//! The `bridgevm` command-line front-end.
//!
//! Forwards native app commands to the packaged app CLI and runs the local
//! own-HVF tools and report commands.

use anyhow::{bail, Context, Result};
use bridgevm_hvf::{probe_virtio_gpu_3d_host_preflight_for, VirtioGpu3dHostPreflightProtocol};
use clap::{Parser, Subcommand, ValueEnum};
use sha2::{Digest, Sha256};
use std::{
    env,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

#[cfg(test)]
mod test_support;

mod app_cli;
mod app_cli_resolver;
mod args;
mod entry;
mod hvf_cmd;
mod report;
mod title_gate;
mod virtio_gpu_trace;

use args::*;
use hvf_cmd::*;
use report::*;
use title_gate::*;
use virtio_gpu_trace::*;

fn main() -> anyhow::Result<()> {
    entry::run()
}
