use super::*;

#[test]
fn pending_messages_drain_without_reallocation_or_replay() {
    let mut platform = BridgeVmPcPlatform::new();
    let message = MsixMessage {
        vector: 3,
        address: board::GIC_MSI_FRAME.base + 0x40,
        data: board::GIC_MSI_INTID_BASE + 3,
    };
    platform.pending_msix.reserve(4);
    platform.pending_msix.push(message);
    let capacity = platform.pending_msix.capacity();
    let mut drained = Vec::new();
    platform.drain_pending_msix_into(&mut drained);
    assert_eq!(drained, [message]);
    assert_eq!(platform.pending_msix.capacity(), capacity);
    platform.drain_pending_msix_into(&mut drained);
    assert_eq!(drained, [message]);
}
