use super::{RawFileDisk, FILE_OVERLAY_CHUNK_SIZE};
use std::io;
use std::os::unix::fs::FileExt;

impl RawFileDisk {
    pub(crate) fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        if self.write_back {
            self.file.write_all_at(data, offset)?;
            return Ok(());
        }

        let mut copied = 0usize;
        while copied < data.len() {
            let abs = offset + copied as u64;
            let chunk_base = (abs / FILE_OVERLAY_CHUNK_SIZE) * FILE_OVERLAY_CHUNK_SIZE;
            let chunk_len = self.chunk_len(chunk_base)?;
            let chunk_off = (abs - chunk_base) as usize;
            let copy_len = (data.len() - copied).min(chunk_len - chunk_off);

            if !self.overlay.contains_key(&chunk_base) {
                let projected = self.overlay_bytes + chunk_len as u64;
                if projected > self.overlay_quota_bytes {
                    return Err(io::Error::new(
                        io::ErrorKind::OutOfMemory,
                        format!(
                            "copy-on-write overlay would reach {projected} bytes, over the {} byte quota",
                            self.overlay_quota_bytes
                        ),
                    ));
                }
                let mut chunk = vec![0u8; chunk_len];
                if chunk_off != 0 || copy_len != chunk_len {
                    #[cfg(test)]
                    {
                        self.overlay_backing_reads += 1;
                    }
                    self.file.read_exact_at(&mut chunk, chunk_base)?;
                }
                self.overlay.insert(chunk_base, chunk);
                self.overlay_bytes = projected;
            }
            let chunk = self.overlay.get_mut(&chunk_base).unwrap();
            chunk[chunk_off..chunk_off + copy_len]
                .copy_from_slice(&data[copied..copied + copy_len]);
            copied += copy_len;
        }
        Ok(())
    }

    pub(crate) fn chunk_len(&self, chunk_base: u64) -> io::Result<usize> {
        if chunk_base >= self.len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "overlay chunk starts past the NVMe image",
            ));
        }
        Ok((self.len - chunk_base).min(FILE_OVERLAY_CHUNK_SIZE) as usize)
    }
}

#[cfg(test)]
mod tests {
    use crate::nvme::disk::DiskBackend;

    #[test]
    fn complete_new_chunks_skip_backing_reads_but_partial_chunks_do_not() {
        let path = std::env::temp_dir().join(format!("bv-cow-full-{}", std::process::id()));
        std::fs::write(&path, vec![0x3c; 8192]).unwrap();
        let mut disk = DiskBackend::raw_file(&path, false).unwrap();
        disk.write_at(1024, &[0xcd; 64]).unwrap();
        let DiskBackend::RawFile(raw) = &mut disk else {
            panic!("expected raw disk")
        };
        assert_eq!(raw.overlay_backing_reads, 1);
        raw.write_at(4096, &[0xef; 4096]).unwrap();
        assert_eq!(raw.overlay_backing_reads, 1);
        assert_eq!(raw.overlay[&4096], [0xef; 4096]);
        let _ = std::fs::remove_file(path);
    }
}
