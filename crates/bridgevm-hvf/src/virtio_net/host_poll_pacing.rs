use super::{NetBackend, VirtioPciNet};
use std::time::{Duration, Instant};

const HOST_SOCKET_POLL_INTERVAL: Duration = Duration::from_millis(1);

#[derive(Debug, Default)]
pub(crate) struct HostPollPacer {
    last_poll: Option<Instant>,
}

impl HostPollPacer {
    pub(crate) fn should_poll(&mut self, now: Option<Instant>) -> bool {
        let Some(now) = now else {
            return true;
        };
        if self
            .last_poll
            .and_then(|last| now.checked_duration_since(last))
            .is_some_and(|elapsed| elapsed < HOST_SOCKET_POLL_INTERVAL)
        {
            return false;
        }
        self.last_poll = Some(now);
        true
    }

    pub(crate) fn reset(&mut self) {
        self.last_poll = None;
    }
}

impl<B: NetBackend> VirtioPciNet<B> {
    pub(crate) fn reset_after_restore(&mut self) {
        self.net.descriptor_scratch.clear();
        self.net.tx_packet_scratch.clear();
        self.net.rx_frame_scratch.clear();
        self.host_poll_pacer.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct CountingBackend {
        host_polls: usize,
    }

    impl NetBackend for CountingBackend {
        fn transmit(&mut self, _frame: &[u8]) {}

        fn poll_receive(&mut self) -> Option<Vec<u8>> {
            None
        }

        fn poll_host_sockets(&mut self) {
            self.host_polls += 1;
        }
    }

    fn retained_trace_polls() -> usize {
        let start = Instant::now();
        let mut pacer = HostPollPacer::default();
        (0..10_000)
            .filter(|step| pacer.should_poll(Some(start + Duration::from_micros(step * 200))))
            .count()
    }

    #[test]
    fn preregistered_trace_reduces_ten_thousand_drains_to_two_thousand_polls() {
        for _ in 0..20 {
            assert_eq!(retained_trace_polls(), 2_000);
        }
        assert_eq!(retained_trace_polls(), 2_000);
    }

    #[test]
    fn missing_time_regression_and_reset_poll_immediately() {
        let start = Instant::now();
        let mut pacer = HostPollPacer::default();
        assert!(pacer.should_poll(Some(start)));
        assert!(!pacer.should_poll(Some(start + Duration::from_micros(999))));
        assert!(pacer.should_poll(None));
        assert!(pacer.should_poll(Some(start - Duration::from_millis(1))));
        pacer.reset();
        assert!(pacer.should_poll(Some(start)));
    }

    #[test]
    fn device_reset_and_snapshot_restore_restart_the_poll_epoch() {
        let start = Instant::now();
        let mut device = VirtioPciNet::new(CountingBackend::default());
        assert!(device.poll_host_sockets(Some(start)));
        assert!(!device.poll_host_sockets(Some(start + Duration::from_micros(200))));
        assert_eq!(device.backend().host_polls, 1);

        let snapshot = device.snapshot_state();
        device.restore_state(&snapshot);
        assert!(device.poll_host_sockets(Some(start + Duration::from_micros(200))));
        assert_eq!(device.backend().host_polls, 2);

        device.reset_runtime_state();
        assert!(device.poll_host_sockets(Some(start + Duration::from_micros(200))));
        assert_eq!(device.backend().host_polls, 3);
    }
}
