//! The BRIDGEVM_HDA_TRACE line for a RUN start, moved out of stream_control.rs
//! unchanged so the RUN rise there could set the stream-activity flag.

use super::*;

impl HdaController {
    pub(crate) fn trace_stream_run(&self) {
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
    }
}
