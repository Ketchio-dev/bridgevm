//! Compatibility entry point for the existing controller unit tests.
#![cfg(test)]

use super::*;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_virt::VirtPlatform;

impl LiveInputController {
    pub fn tick(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
        event_wake: bool,
    ) {
        if self.prepare_tick(now, event_wake) {
            self.apply_pending(platform, mem, now);
        }
    }
}
