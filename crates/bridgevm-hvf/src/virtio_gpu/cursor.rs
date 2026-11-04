//! The guest's hardware cursor: UPDATE_CURSOR and MOVE_CURSOR from the
//! cursor queue, published to the app through `display.cursor`
//! (cursor_file.rs), which draws it at the host pointer.
//!
//! QEMU's semantics (`update_cursor`): the sprite is a 64x64 2D resource
//! whose host copy TRANSFER_TO_HOST_2D has already filled, so no guest memory
//! is read here, and a command naming resource 0 hides it. The guest-visible
//! side is unchanged: every command is acknowledged as before.

use super::cursor_file::{CursorFile, CURSOR_IMAGE_BYTES, CURSOR_SIZE};
use super::{read_le_u32, GpuResource, FORMAT_B8G8R8A8_UNORM, VIRTIO_GPU_CMD_MOVE_CURSOR};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// struct virtio_gpu_update_cursor: header 24, pos (scanout_id, x, y,
/// padding) 16, resource_id, hot_x, hot_y, padding.
const UPDATE_CURSOR_LEN: usize = 56;

/// What `display.cursor` says; see cursor_file.rs for the layout.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CursorState {
    pub(crate) serial: u32,
    pub(crate) visible: bool,
    pub(crate) shape_this_boot: bool,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) hot_x: u32,
    pub(crate) hot_y: u32,
}

#[derive(Debug, Default)]
pub(crate) struct GuestCursor {
    file: Option<CursorFile>,
    pub(crate) state: CursorState,
    pub(crate) updates: u64,
    pub(crate) moves: u64,
    /// Times the sprite went from shown to hidden.
    pub(crate) hides: u64,
}

impl GuestCursor {
    /// The cursor file next to `BRIDGEVM_DISPLAY_EXPORT_FB`; none without it.
    pub(crate) fn from_env() -> Self {
        std::env::var_os("BRIDGEVM_DISPLAY_EXPORT_FB")
            .filter(|path| !path.is_empty())
            .map(|path| Self::at_path(&PathBuf::from(path).with_extension("cursor")))
            .unwrap_or_default()
    }

    /// Publishes to `path` at once, hidden and with no shape for this boot,
    /// so a file left by an earlier boot never shows its cursor. Without the
    /// file the cursor is only tracked; the app then shows the macOS pointer.
    pub(crate) fn at_path(path: &Path) -> Self {
        match CursorFile::open(path) {
            Ok((file, serial)) => {
                let mut cursor = Self {
                    file: Some(file),
                    state: CursorState {
                        serial,
                        ..CursorState::default()
                    },
                    ..Self::default()
                };
                cursor.publish(None);
                cursor
            }
            Err(error) => {
                eprintln!(
                    "virtio-gpu cursor export is off ({}): {error}; the app shows the macOS pointer instead",
                    path.display()
                );
                Self::default()
            }
        }
    }

    /// A device reset takes the guest's driver, and its sprite, away.
    pub(crate) fn reset(&mut self) {
        self.set_visible(false);
        self.publish(None);
    }

    /// One UPDATE_CURSOR or MOVE_CURSOR. A short request, or one for a
    /// scanout other than 0 (the only one), changes nothing, as in QEMU.
    pub(crate) fn command(
        &mut self,
        typ: u32,
        request: &[u8],
        resources: &BTreeMap<u32, GpuResource>,
    ) {
        if request.len() < UPDATE_CURSOR_LEN || read_le_u32(request, 24) != Some(0) {
            return;
        }
        let word = |offset| read_le_u32(request, offset).unwrap_or(0);
        let (x, y, resource_id) = (word(28) as i32, word(32) as i32, word(40));
        let origin = x == 0 && y == 0;
        let mut image = None;
        if typ == VIRTIO_GPU_CMD_MOVE_CURSOR {
            self.moves += 1;
            // viogpudo up to the pinned 0.1-299 build hides its sprite by
            // moving it to (0,0) with the resource still attached (upstream
            // 8972250b now sends UPDATE_CURSOR with resource 0). A move to
            // exactly (0,0) therefore counts as hidden. The trade-off: the
            // cursor vanishes while the guest pointer really sits on pixel
            // (0,0), and comes back with the next move off it.
            if !origin {
                (self.state.x, self.state.y) = (x, y);
            }
            self.set_visible(resource_id != 0 && !origin);
        } else {
            self.updates += 1;
            if let Some(resource) = resources.get(&resource_id).filter(|r| is_cursor_image(r)) {
                image = Some(resource.host_pixels.as_slice());
                self.state.serial = self.state.serial.wrapping_add(1);
                self.state.shape_this_boot = true;
                self.state.hot_x = word(44).min(CURSOR_SIZE - 1);
                self.state.hot_y = word(48).min(CURSOR_SIZE - 1);
            }
            // viogpudo sends (0,0) with every new shape, and before upstream
            // b4b33db2 nothing orders that UPDATE after a concurrent
            // MOVE_CURSOR, so (0,0) keeps the last position.
            if !origin {
                (self.state.x, self.state.y) = (x, y);
            }
            self.set_visible(resource_id != 0);
        }
        self.publish(image);
    }

    /// Shown only with an image; counts each shown-to-hidden change.
    fn set_visible(&mut self, visible: bool) {
        let visible = visible && self.state.shape_this_boot;
        if self.state.visible && !visible {
            self.hides += 1;
        }
        self.state.visible = visible;
    }

    fn publish(&mut self, image: Option<&[u8]>) {
        if let Some(file) = self.file.as_mut() {
            file.publish(&self.state, image);
        }
    }
}

/// A cursor image the app can take as is: exactly 64x64 with alpha. Any
/// other resource keeps the previous image, as in QEMU.
fn is_cursor_image(resource: &GpuResource) -> bool {
    resource.width == CURSOR_SIZE
        && resource.height == CURSOR_SIZE
        && resource.format == FORMAT_B8G8R8A8_UNORM
        && resource.host_pixels.len() == CURSOR_IMAGE_BYTES
}

#[cfg(test)]
#[path = "cursor_tests.rs"]
mod tests;
