//! An attempt leaves its directory identity before writing any snapshot data.

use super::{identity, sync_dir};
use crate::snapshot_pair::files::open_regular;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub(super) const NAME: &str = ".bridgevm-create-owner";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub(super) stage: (u64, u64),
    pub(super) reservation: Option<(u64, u64)>,
}

impl Record {
    pub(super) fn write(&self, stage: &Path) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(stage.join(NAME))?;
        file.write_all(&serde_json::to_vec(self).map_err(io::Error::other)?)?;
        file.sync_all()?;
        sync_dir(stage)
    }

    pub(super) fn read(stage: &Path) -> io::Result<Self> {
        // The record holds only two pairs of 64-bit integers.
        let mut data = Vec::new();
        open_regular(&stage.join(NAME))?
            .take(256)
            .read_to_end(&mut data)?;
        let record: Self = serde_json::from_slice(&data).map_err(io::Error::other)?;
        if data.len() == 256 || record.stage.1 == 0 || identity(stage)? != record.stage {
            return Err(io::Error::other("staging ownership record does not match"));
        }
        Ok(record)
    }
}
