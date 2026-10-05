//! Owned raw-file fixtures for the write completion durability boundary.

use super::super::*;
use super::helpers::*;
use super::write_completion_files::OwnedFiles;
use crate::fwcfg::GuestMemoryMut;
use std::{fs, io};

pub(super) struct WriteFixture {
    pub ctrl: NvmeController,
    pub mem: FakeMem,
    files: OwnedFiles,
}

impl WriteFixture {
    pub fn new(direct: bool) -> Self {
        let files = OwnedFiles::new();
        let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&files.paths[0], true, 0x9000);
        ctrl.attach_second_namespace_raw_file(&files.paths[1], true)
            .unwrap();
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);
        if direct {
            mem.enable_host_ptr();
        }
        assert!(mem.write_bytes(DATA_BASE, &[0x73; LBA_SIZE]));
        Self { ctrl, mem, files }
    }

    pub fn cache(&mut self, slot: u16, enabled: bool) {
        let cmd = encode_sqe(
            ADMIN_OP_SET_FEATURES,
            0x60 + slot,
            0,
            0,
            u32::from(FEATURE_VOLATILE_WRITE_CACHE),
            u32::from(enabled),
            0,
        );
        submit_admin(&mut self.ctrl, &mut self.mem, slot, &cmd);
        let cqe = read_completion(&self.mem, ACQ_BASE, slot);
        assert_eq!(&cqe[12..14], &cmd[2..4]);
        assert_eq!(cqe[14] & 1, 1);
        assert_eq!(completion_status(&cqe), SC_SUCCESS);
    }

    pub fn counters(&self) -> [usize; 2] {
        [
            raw_file_sync_attempts(&self.ctrl.disk),
            raw_file_sync_attempts(self.ctrl.disk2.as_ref().unwrap()),
        ]
    }

    pub fn fail_sync(&mut self, nsid: u32) {
        set_raw_file_sync_failure(
            self.ctrl.backend_for_nsid_mut(nsid).unwrap(),
            Some(io::ErrorKind::Other),
        );
    }

    pub fn fail_write(&mut self, nsid: u32) {
        let file = fs::File::open(&self.files.paths[(nsid - 1) as usize]).unwrap();
        let DiskBackend::RawFile(raw) = self.ctrl.backend_for_nsid_mut(nsid).unwrap() else {
            panic!("raw file required");
        };
        // Keep write_back=true but use a read-only owned descriptor: pwrite fails.
        raw.file = file;
    }

    pub fn write(&mut self, nsid: u32, fua: bool, prp: u64, lba: u32) -> u16 {
        let cmd = encode_sqe(
            NVM_OP_WRITE,
            0x91,
            nsid,
            prp,
            lba,
            0,
            if fua { 1 << 30 } else { 0 },
        );
        let status = submit_io(&mut self.ctrl, &mut self.mem, 0, &cmd);
        let cqe = read_completion(&self.mem, IO_CQ_BASE, 0);
        assert_eq!(&cqe[12..14], &cmd[2..4]);
        assert_eq!(cqe[14] & 1, 1);
        status
    }

    pub fn assert_bytes(&self, selected: u32, wrote: bool) {
        for nsid in [NSID, NSID2] {
            let bytes = fs::read(&self.files.paths[(nsid - 1) as usize]).unwrap();
            let pattern = if nsid == selected && wrote { 0x73 } else { 0 };
            assert_eq!(&bytes[..LBA_SIZE], &[pattern; LBA_SIZE]);
            assert!(bytes[LBA_SIZE..].iter().all(|byte| *byte == 0));
        }
    }
}
