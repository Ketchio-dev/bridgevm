//! Share mount/unmount and file-drop start/chunk/complete, with safe destination and base64 decode.

use crate::*;
use anyhow::Result;
use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn safe_file_drop_destination(root: &Path, file_name: &str) -> Option<PathBuf> {
    let mut components = Path::new(file_name).components();
    let Some(Component::Normal(name)) = components.next() else {
        return None;
    };
    if components.next().is_some() {
        return None;
    }
    Some(root.join(name))
}

pub(crate) fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let bytes = input.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err("base64 payload length must be a multiple of 4".to_string());
    }

    let mut output = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut index = 0usize;
    while index < bytes.len() {
        let chunk = &bytes[index..index + 4];
        let mut values = [0_u8; 4];
        let mut padding = 0usize;
        for (offset, byte) in chunk.iter().enumerate() {
            if *byte == b'=' {
                padding += 1;
                values[offset] = 0;
                continue;
            }
            if padding > 0 {
                return Err("base64 padding must be at the end of the payload".to_string());
            }
            values[offset] = decode_base64_value(*byte)
                .ok_or_else(|| format!("base64 payload contains invalid byte 0x{byte:02x}"))?;
        }
        if padding > 2 {
            return Err("base64 payload has too much padding".to_string());
        }
        if padding > 0 && index + 4 != bytes.len() {
            return Err("base64 padding is only allowed in the final chunk".to_string());
        }

        output.push((values[0] << 2) | (values[1] >> 4));
        if padding < 2 {
            output.push((values[1] << 4) | (values[2] >> 2));
        }
        if padding == 0 {
            output.push((values[2] << 6) | values[3]);
        }
        index += 4;
    }

    Ok(output)
}

pub(crate) fn decode_base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

impl GuestToolsState {
    pub(crate) fn mount_share(&mut self, name: &str, host_path_token: &str) -> CommandOutcome {
        if !self.shared_folders_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "shared folders capability is not enabled",
            );
        }

        let existed = self.shared_folders.insert(
            name.to_string(),
            SharedFolderMount {
                host_path_token: host_path_token.to_string(),
            },
        );
        if existed.is_some() {
            CommandOutcome::ok(Some(format!("accepted share update for {name}")))
        } else {
            CommandOutcome::ok(Some(format!("accepted mount request for share {name}")))
        }
    }

    pub(crate) fn unmount_share(&mut self, name: &str) -> CommandOutcome {
        if !self.shared_folders_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "shared folders capability is not enabled",
            );
        }

        if self.shared_folders.remove(name).is_some() {
            CommandOutcome::ok(Some(format!("accepted unmount request for share {name}")))
        } else {
            CommandOutcome::error("share-not-mounted", format!("share {name} is not mounted"))
        }
    }

    pub(crate) fn start_file_drop(
        &mut self,
        transfer_id: &str,
        file_name: &str,
        size_bytes: u64,
    ) -> CommandOutcome {
        if !self.drag_drop_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "drag-and-drop capability is not enabled",
            );
        }
        if self.file_drops.contains_key(transfer_id) {
            return CommandOutcome::error(
                "transfer-already-started",
                format!("file drop {transfer_id} is already active"),
            );
        }

        self.file_drops.insert(
            transfer_id.to_string(),
            FileDropTransfer {
                file_name: file_name.to_string(),
                size_bytes,
                bytes: Vec::new(),
                chunks_seen: 0,
            },
        );
        CommandOutcome::ok(Some(format!("started file drop {transfer_id}")))
    }

    pub(crate) fn record_file_drop_chunk(
        &mut self,
        transfer_id: &str,
        chunk_index: u32,
        data_base64: &str,
    ) -> CommandOutcome {
        if !self.drag_drop_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "drag-and-drop capability is not enabled",
            );
        }
        let Some(transfer) = self.file_drops.get_mut(transfer_id) else {
            return CommandOutcome::error(
                "transfer-not-started",
                format!("file drop {transfer_id} has not started"),
            );
        };
        let chunk = match decode_base64(data_base64) {
            Ok(chunk) => chunk,
            Err(message) => return CommandOutcome::error("invalid-file-drop-chunk", message),
        };

        // Reject as soon as the accumulated bytes would exceed the size declared
        // in FileDropStart, so a misbehaving/compromised sender can't grow this
        // buffer without bound (duplicate or oversized chunks) before the final
        // size check in complete_file_drop.
        if transfer.bytes.len() as u64 + chunk.len() as u64 > transfer.size_bytes {
            return CommandOutcome::error(
                "file-drop-overflow",
                format!(
                    "file drop {transfer_id} chunk {chunk_index} exceeds declared size {}",
                    transfer.size_bytes
                ),
            );
        }

        transfer.bytes.extend_from_slice(&chunk);
        transfer.chunks_seen = transfer.chunks_seen.max(chunk_index.saturating_add(1));
        CommandOutcome::ok(Some(format!(
            "accepted file drop {transfer_id} chunk {chunk_index}"
        )))
    }

    pub(crate) fn complete_file_drop(&mut self, transfer_id: &str) -> CommandOutcome {
        if !self.drag_drop_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "drag-and-drop capability is not enabled",
            );
        }
        let Some(transfer) = self.file_drops.get(transfer_id) else {
            return CommandOutcome::error(
                "transfer-not-started",
                format!("file drop {transfer_id} has not started"),
            );
        };
        if transfer.bytes.len() as u64 != transfer.size_bytes {
            return CommandOutcome::error(
                "transfer-size-mismatch",
                format!(
                    "file drop {} expected {} bytes but received {}",
                    transfer.file_name,
                    transfer.size_bytes,
                    transfer.bytes.len()
                ),
            );
        }
        if let Some(file_drop_dir) = &self.file_drop_dir {
            let Some(destination) = safe_file_drop_destination(file_drop_dir, &transfer.file_name)
            else {
                return CommandOutcome::error(
                    "unsafe-file-name",
                    format!("file drop file name is not safe: {}", transfer.file_name),
                );
            };
            if let Err(error) = fs::create_dir_all(file_drop_dir) {
                return CommandOutcome::error(
                    "file-drop-write-failed",
                    format!(
                        "failed to create file drop directory {}: {error}",
                        file_drop_dir.display()
                    ),
                );
            }
            if let Err(error) = fs::write(&destination, &transfer.bytes) {
                return CommandOutcome::error(
                    "file-drop-write-failed",
                    format!(
                        "failed to write file drop {}: {error}",
                        destination.display()
                    ),
                );
            }
        }
        let transfer = self
            .file_drops
            .remove(transfer_id)
            .expect("transfer was checked above");

        let mut message = format!(
            "completed file drop {} ({} bytes across {} chunks)",
            transfer.file_name, transfer.size_bytes, transfer.chunks_seen
        );
        if let Some(file_drop_dir) = &self.file_drop_dir {
            if let Some(destination) =
                safe_file_drop_destination(file_drop_dir, &transfer.file_name)
            {
                message.push_str(&format!(" at {}", destination.display()));
            }
        }
        CommandOutcome::ok(Some(message))
    }
}
