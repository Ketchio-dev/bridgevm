//! Checkpoint serialization and restore of controller, queue, and MSI-X state.

use super::*;

impl NvmeController {
    pub fn snapshot_state(&self) -> Vec<u8> {
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_u32(self.cc);
        out.write_u32(self.csts);
        out.write_u32(self.aqa);
        out.write_u64(self.asq);
        out.write_u64(self.acq);
        out.write_u32(self.intms);
        out.write_u16(self.max_io_queues);
        out.write_u16(0);
        out.write_u32(self.last_feature_result);
        out.write_bool(self.volatile_write_cache_enabled);
        out.write_bool(self.direct_dma_enabled);
        out.write_u8(self.pending_async_event_requests);
        out.write_u8(0);

        out.write_u32(self.sqs.len() as u32);
        for queue in &self.sqs {
            out.write_bool(queue.is_some());
            if let Some(queue) = queue {
                out.write_u64(queue.base);
                out.write_u16(queue.size);
                out.write_u16(queue.head);
                out.write_u16(queue.tail_doorbell);
                out.write_u16(queue.cqid);
            }
        }

        out.write_u32(self.cqs.len() as u32);
        for queue in &self.cqs {
            out.write_bool(queue.is_some());
            if let Some(queue) = queue {
                out.write_u64(queue.base);
                out.write_u16(queue.size);
                out.write_u16(queue.tail);
                out.write_bool(queue.phase);
                out.write_bool(queue.interrupts_enabled);
                out.write_u16(queue.head);
                out.write_u16(queue.interrupt_vector);
            }
        }

        out.write_u32(self.pending_sq_bits.len() as u32);
        for word in &self.pending_sq_bits {
            out.write_u64(*word);
        }

        out.write_blob(&self.msix.snapshot_state());
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(input.read_u32(), 1, "unsupported NVMe snapshot version");

        self.cc = input.read_u32();
        self.csts = input.read_u32();
        self.aqa = input.read_u32();
        self.asq = input.read_u64();
        self.acq = input.read_u64();
        self.intms = input.read_u32();
        self.max_io_queues = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid NVMe snapshot");
        self.last_feature_result = input.read_u32();
        self.volatile_write_cache_enabled = input.read_bool();
        self.direct_dma_enabled = input.read_bool();
        self.pending_async_event_requests = input.read_u8();
        assert_eq!(input.read_u8(), 0, "invalid NVMe snapshot");

        let sq_count = input.read_u32() as usize;
        assert!(
            sq_count <= MAX_IO_QUEUE_PAIRS as usize + 1,
            "NVMe SQ count exceeds controller capacity"
        );
        self.sqs.clear();
        self.sqs.reserve(sq_count);
        for _ in 0..sq_count {
            self.sqs.push(if input.read_bool() {
                Some(SubmissionQueue {
                    base: input.read_u64(),
                    size: input.read_u16(),
                    head: input.read_u16(),
                    tail_doorbell: input.read_u16(),
                    cqid: input.read_u16(),
                })
            } else {
                None
            });
        }

        let cq_count = input.read_u32() as usize;
        assert!(
            cq_count <= MAX_IO_QUEUE_PAIRS as usize + 1,
            "NVMe CQ count exceeds controller capacity"
        );
        self.cqs.clear();
        self.cqs.reserve(cq_count);
        for _ in 0..cq_count {
            self.cqs.push(if input.read_bool() {
                Some(CompletionQueue {
                    base: input.read_u64(),
                    size: input.read_u16(),
                    tail: input.read_u16(),
                    phase: input.read_bool(),
                    interrupts_enabled: input.read_bool(),
                    head: input.read_u16(),
                    interrupt_vector: input.read_u16(),
                })
            } else {
                None
            });
        }

        let pending_words = input.read_u32() as usize;
        self.pending_sq_bits.clear();
        self.pending_sq_bits.reserve(pending_words);
        for _ in 0..pending_words {
            self.pending_sq_bits.push(input.read_u64());
        }

        self.msix.restore_state(&input.read_blob());

        self.command_trace.clear();
        self.io_scratch.clear();
        self.prp_spans_scratch.clear();
        self.io_segments_scratch.clear();
        input.finish();
    }
}
