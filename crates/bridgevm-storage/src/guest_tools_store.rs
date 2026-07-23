//! Guest-tools token minting and persistence, and runner/runtime metadata.

use crate::*;
use bridgevm_qemu::guest_tools_socket_path;
use std::fs;
use std::io::Read;
use std::path::Path;

pub(crate) fn new_guest_tools_token() -> Result<GuestToolsTokenMetadata, StorageError> {
    let mut bytes = [0_u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(GuestToolsTokenMetadata {
        token: hex_encode(&bytes),
        created_at_unix: now_unix(),
    })
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

pub(crate) const GUEST_TOOLS_CHANNEL_NAME: &str = "org.bridgevm.guest-tools.0";

impl VmStore {
    pub fn guest_tools_token(
        &self,
        vm_name: &str,
    ) -> Result<GuestToolsTokenMetadata, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        self.guest_tools_token_at(&bundle)
    }

    pub fn guest_tools_runner_metadata(
        &self,
        vm_name: &str,
    ) -> Result<GuestToolsRunnerMetadata, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let token = self.guest_tools_token_at(&bundle)?;
        Ok(GuestToolsRunnerMetadata {
            transport: "virtio-serial".to_string(),
            channel_name: GUEST_TOOLS_CHANNEL_NAME.to_string(),
            socket_path: guest_tools_socket_path(&bundle),
            token_path: guest_tools_token_path(&bundle),
            token_created_at_unix: token.created_at_unix,
        })
    }

    pub fn guest_tools_runtime_metadata(
        &self,
        vm_name: &str,
    ) -> Result<Option<GuestToolsRuntimeMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = guest_tools_runtime_path(&bundle);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    pub fn write_guest_tools_runtime_metadata(
        &self,
        vm_name: &str,
        metadata: &GuestToolsRuntimeMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        write_json_pretty_atomic(&guest_tools_runtime_path(&bundle), metadata)
    }

    pub(crate) fn guest_tools_token_at(
        &self,
        bundle: &Path,
    ) -> Result<GuestToolsTokenMetadata, StorageError> {
        let path = guest_tools_token_path(bundle);
        if path.exists() {
            return read_json_required(&path);
        }

        let metadata = new_guest_tools_token()?;
        self.write_guest_tools_token_at(bundle, &metadata)?;
        Ok(metadata)
    }

    pub(crate) fn write_guest_tools_token_at(
        &self,
        bundle: &Path,
        metadata: &GuestToolsTokenMetadata,
    ) -> Result<(), StorageError> {
        write_json_pretty_atomic(&guest_tools_token_path(bundle), metadata)
    }
}
