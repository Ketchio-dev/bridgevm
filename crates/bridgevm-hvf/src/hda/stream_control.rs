//! Playback stream control and pause/resume semantics.

use super::*;

impl HdaController {
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
            self.stream_activity.stop();
            self.notify_stream_stopped(was_running);
            self.notify_stream_reset();
            return;
        }
        self.stream.ctl = next & !SDCTL_SRST;
        let running = self.stream.ctl & SDCTL_RUN != 0;
        if running && !was_running {
            // RUN resumes DMA without acknowledging status or resetting the cursor.
            self.last_poll = None;
            self.byte_time_remainder = 0;
            self.stream_activity.start();
            self.trace_stream_run();
        } else if !running {
            self.last_poll = None;
            self.stream_activity.stop();
            self.notify_stream_stopped(was_running);
        }
    }
}
