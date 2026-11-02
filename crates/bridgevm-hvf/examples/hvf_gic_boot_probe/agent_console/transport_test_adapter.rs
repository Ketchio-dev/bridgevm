//! Legacy test entry points adapt to the same production sender path.

use crate::agent_console::*;
use bridgevm_hvf::fwcfg::GuestMemoryMut;

fn with_sender<R>(
    mut platform: Option<&mut VirtPlatform>,
    mut mem: Option<&mut dyn GuestMemoryMut>,
    body: impl FnOnce(&mut dyn FnMut(&[u8])) -> R,
) -> R {
    body(&mut |bytes| {
        if let (Some(platform), Some(mem)) = (platform.as_mut(), mem.as_mut()) {
            platform.virtio_console_agent_send(bytes, &mut **mem);
        }
    })
}

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn service_pump(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) {
        with_sender(Some(platform), Some(mem), |send| {
            self.service_pump_with_sender(now, send)
        });
    }

    pub(in crate::agent_console) fn handle_service_reply(
        &mut self,
        line: &str,
        platform: Option<&mut VirtPlatform>,
        mem: Option<&mut dyn GuestMemoryMut>,
        now: Instant,
    ) {
        with_sender(platform, mem, |send| {
            self.handle_service_reply_with_sender(line, now, send)
        });
    }

    pub(in crate::agent_console) fn handle_share_put_reply(
        &mut self,
        line: &str,
        platform: Option<&mut VirtPlatform>,
        mem: Option<&mut dyn GuestMemoryMut>,
        now: Instant,
    ) {
        with_sender(platform, mem, |send| {
            self.handle_share_put_reply_with_sender(line, now, send)
        });
    }
}
