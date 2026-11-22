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
mod json_util;
mod report;
mod title_gate;

use args::*;
use hvf_cmd::*;
use json_util::*;
use report::*;
use title_gate::*;

fn main() -> anyhow::Result<()> {
    entry::run()
}
