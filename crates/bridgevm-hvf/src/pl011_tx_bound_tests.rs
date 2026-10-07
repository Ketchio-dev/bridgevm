use super::*;

#[test]
fn undrained_transmit_log_stops_growing_at_its_bound() {
    let mut uart = Pl011::new();
    for _ in 0..MAX_RETAINED_TX_LEN + 1024 {
        uart.mmio_write(UARTDR, 1, u64::from(b'A'));
    }
    assert_eq!(uart.output().len(), MAX_RETAINED_TX_LEN);

    // Draining the log, as the KD serial bridge does, makes room again.
    assert_eq!(uart.take_output().len(), MAX_RETAINED_TX_LEN);
    uart.mmio_write(UARTDR, 1, u64::from(b'B'));
    assert_eq!(uart.output(), b"B");
}
