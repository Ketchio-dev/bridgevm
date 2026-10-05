//! Stream and controller reset semantics, and the host sink's stop notice.

use super::*;

impl HdaController {
    pub(crate) fn controller_reset(&mut self) {
        self.notify_stream_stopped(self.stream.ctl & SDCTL_RUN != 0);
        let pcm_sink = self.pcm_sink.take();
        let pcm_sink_overridden = self.pcm_sink_overridden;
        *self = Self::with_pcm_output_path::<&Path>(None);
        self.pcm_sink = pcm_sink;
        self.pcm_sink_overridden = pcm_sink_overridden;
    }

    pub(crate) fn write_stream_ctl(&mut self, next: u32) {
        let was_running = self.stream.ctl & SDCTL_RUN != 0;
        if next & SDCTL_SRST != 0 {
            let format = self.stream.fmt;
            let bdl = self.stream.bdl;
            let cbl = self.stream.cbl;
            let lvi = self.stream.lvi;
            self.stream = StreamDescriptor {
                ctl: SDCTL_SRST,
                fmt: format,
                bdl,
                cbl,
                lvi,
                ..StreamDescriptor::default()
            };
            self.last_poll = None;
            self.byte_time_remainder = 0;
            self.notify_stream_stopped(was_running);
            return;
        }
        self.stream.ctl = next & !SDCTL_SRST;
        let running = self.stream.ctl & SDCTL_RUN != 0;
        if running && !was_running {
            // RUN resumes DMA without acknowledging status or resetting the cursor.
            self.last_poll = None;
            self.byte_time_remainder = 0;
            if hda_trace_enabled() {
                println!(
                    "hda: stream run fmt={:#06x} rate={}Hz frame={} BDL={:#x} CBL={} LVI={}",
                    self.stream.fmt,
                    stream_sample_rate(self.stream.fmt).unwrap_or(0),
                    stream_frame_bytes(self.stream.fmt).unwrap_or(0),
                    self.stream.bdl,
                    self.stream.cbl,
                    self.stream.lvi
                );
            }
        } else if !running {
            self.last_poll = None;
            self.notify_stream_stopped(was_running);
        }
    }

    /// A guest stop of a running stream reaches the host sink (HdaPcmSink::stream_stopped).
    fn notify_stream_stopped(&mut self, was_running: bool) {
        if let (true, Some(sink)) = (was_running, self.pcm_sink.as_mut()) {
            sink.stream_stopped();
        }
    }
}
