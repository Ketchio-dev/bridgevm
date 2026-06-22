use super::test_support::{setup_packet_parameter, SetupPacketFields, DOORBELL_BASE};
use super::*;
use crate::fwcfg::GuestMemoryMut;

const HIGH_EP0_RING: u64 = u64::MAX - 7;
const TRB_CYCLE: u32 = 1;
const TRB_TYPE_SETUP_STAGE: u32 = 2;

#[test]
fn ep0_control_transfer_rejects_overflowing_ring_arithmetic_without_panic() {
    // Given: a malicious guest points EP0 at a readable setup TRB near u64::MAX.
    let mut xhci = XhciController::new();
    let mut mem = HighSetupOnlyRam;
    xhci.slot1_ep0_dequeue = HIGH_EP0_RING;

    // When: the controller tries to process the EP0 control transfer.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem)
    }));

    // Then: overflow is rejected as a normal failed transfer path, not a panic.
    assert!(matches!(result, Ok(false)));
    assert_eq!(xhci.slot1_ep0_dequeue, HIGH_EP0_RING);
}

struct HighSetupOnlyRam;

impl GuestMemoryMut for HighSetupOnlyRam {
    fn write_bytes(&mut self, _gpa: u64, _data: &[u8]) -> bool {
        false
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        if gpa != HIGH_EP0_RING || len != 16 {
            return None;
        }
        let mut trb = vec![0; 16];
        trb[0..8].copy_from_slice(
            &setup_packet_parameter(SetupPacketFields {
                bm_request_type: 0x80,
                request: 0x08,
                value: 0,
                index: 0,
                length: 1,
            })
            .to_le_bytes(),
        );
        trb[8..12].copy_from_slice(&8u32.to_le_bytes());
        trb[12..16].copy_from_slice(&transfer_control(TRB_TYPE_SETUP_STAGE).to_le_bytes());
        Some(trb)
    }
}

fn transfer_control(trb_type: u32) -> u32 {
    (trb_type << 10) | TRB_CYCLE
}
