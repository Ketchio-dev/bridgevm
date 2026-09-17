//! Publication of the composited scanout to the app framebuffer sink.

use super::*;
use crate::ramfb::DRM_FORMAT_XRGB8888;

impl VirtioGpu {
    pub(crate) fn publish_scanout_fb(&mut self) {
        if self.scanout_resource.is_none() && self.blob_scanout.is_none() {
            return;
        }
        self.publish_scanout_fb_unconditionally();
    }

    pub(super) fn publish_scanout_fb_damage(&mut self, damage: Rect) {
        let width = self.width;
        let height = self.height;
        let stride = width * 4;
        if self.scanout.len() < (stride as usize) * (height as usize) {
            return;
        }
        let (fb_sink, scanout) = (&mut self.fb_sink, &self.scanout);
        if let Some(sink) = fb_sink.as_mut() {
            sink.write_damage(width, height, stride, DRM_FORMAT_XRGB8888, scanout, damage);
        }
    }

    /// Publish checkpointed pixels even when the producing blob scanout cannot
    /// be serialized. This prevents a black restore while WDDM re-establishes it.
    pub(crate) fn publish_scanout_fb_unconditionally(&mut self) {
        let width = self.width;
        let height = self.height;
        let stride = width * 4;
        if self.scanout.len() < (stride as usize) * (height as usize) {
            return;
        }
        let (fb_sink, scanout) = (&mut self.fb_sink, &self.scanout);
        if let Some(sink) = fb_sink.as_mut() {
            sink.write(width, height, stride, DRM_FORMAT_XRGB8888, scanout);
        }
    }
}
