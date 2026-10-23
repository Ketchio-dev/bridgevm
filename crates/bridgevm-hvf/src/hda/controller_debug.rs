//! HdaController's Debug output, moved out of hdapcmsink.rs unchanged to make
//! room there for the stream-activity handle (stream_activity.rs).

use super::*;

impl std::fmt::Debug for HdaController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HdaController")
            .field("gctl", &self.gctl)
            .field("statests", &self.statests)
            .field("intctl", &self.intctl)
            .field("corb_base", &self.corb_base)
            .field("corb_wp", &self.corb_wp)
            .field("corb_rp", &self.corb_rp)
            .field("rirb_base", &self.rirb_base)
            .field("rirb_wp", &self.rirb_wp)
            .field("stream", &self.stream)
            .field("pcm_sink", &self.pcm_sink.is_some())
            .field("interrupt_asserted", &self.interrupt_asserted)
            .field("interrupt_pending", &self.interrupt_pending)
            .finish()
    }
}
