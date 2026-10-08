//! Host sink notifications distinguish a pause from a terminal PCM generation.

use super::*;

impl HdaController {
    pub(crate) fn controller_reset(&mut self) {
        self.notify_stream_stopped(self.stream.ctl & SDCTL_RUN != 0);
        self.notify_stream_reset();
        let pcm_sink = self.pcm_sink.take();
        let pcm_sink_overridden = self.pcm_sink_overridden;
        *self = Self::with_pcm_output_path::<&Path>(None);
        self.pcm_sink = pcm_sink;
        self.pcm_sink_overridden = pcm_sink_overridden;
    }

    pub(crate) fn write_stream_format(&mut self, next: u16) {
        let format = next & 0x7fff;
        if self.stream.ctl & SDCTL_RUN == 0 && self.stream.fmt != format {
            self.stream.fmt = format;
            self.notify_stream_reset();
        }
    }

    pub(crate) fn notify_stream_stopped(&mut self, was_running: bool) {
        if let (true, Some(sink)) = (was_running, self.pcm_sink.as_mut()) {
            sink.stream_stopped();
        }
    }

    pub(crate) fn notify_stream_reset(&mut self) {
        if let Some(sink) = self.pcm_sink.as_mut() {
            sink.stream_reset();
        }
    }
}
