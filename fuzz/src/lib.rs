//! Fuzz target bodies, shared between `cargo fuzz` and the stable smoke
//! runner.
//!
//! Keeping the bodies here rather than inside `fuzz_target!` means the
//! checked-in corpus exercises exactly the code libFuzzer would, on a machine
//! where cargo-fuzz and a nightly toolchain are not installed. A seed that
//! only ever ran under a tool nobody has is not a regression test.
//!
//! Every target must accept arbitrary bytes and either parse them or decline.
//! A panic is a finding: these inputs are guest-controlled.

use bridgevm_hvf::net_nat::{EthernetFrame, Ipv4Packet, NatBackend, TcpSegment, UdpDatagram};
use bridgevm_hvf::nvme::SubmissionEntry;
use bridgevm_hvf::virtio_net::NetBackend as _;
use bridgevm_hvf::virtio_gpu_3d::CtrlHdr3d;

/// NVMe submission-queue entry decode. The entry is a fixed 64 bytes read
/// from guest RAM, so short inputs are padded rather than rejected: the device
/// would face the same zero-filled tail.
pub fn nvme_prp(data: &[u8]) {
    let mut entry = [0u8; 64];
    let take = data.len().min(64);
    entry[..take].copy_from_slice(&data[..take]);
    let decoded = SubmissionEntry::from_bytes(&entry);
    // Touch the decoded fields so the decode cannot be optimised away.
    std::hint::black_box(decoded.opcode);
}

/// virtio-gpu 3D control header parse.
pub fn virtqueue_chain(data: &[u8]) {
    if let Some(header) = CtrlHdr3d::parse(data) {
        std::hint::black_box((header.typ, header.ctx_id, header.ring_idx));
    }
}

/// fw_cfg DMA descriptor decode. Named for the TRB target in PLAN.md; the
/// xHCI TRB decoder is not public, and inventing an export to make a target
/// compile would widen the API for a test's convenience.
pub fn xhci_trb(data: &[u8]) {
    let mut descriptor = [0u8; 16];
    let take = data.len().min(16);
    descriptor[..take].copy_from_slice(&data[..take]);
    let decoded = bridgevm_hvf::fwcfg::FwCfgDmaAccess::from_bytes(&descriptor);
    std::hint::black_box((decoded.control, decoded.length));
}

pub fn nat_packet(data: &[u8]) {
    let mut backend = NatBackend::new(); backend.transmit(data); std::hint::black_box(backend.stats());
    if let Some(frame) = EthernetFrame::parse(data) {
        std::hint::black_box(frame.ethertype);
    }
    if let Some(packet) = Ipv4Packet::parse(data) {
        std::hint::black_box((packet.protocol, packet.total_len));
    }
    if let Some(segment) = TcpSegment::parse(data) {
        std::hint::black_box((segment.src_port, segment.dst_port));
    }
    if let Some(datagram) = UdpDatagram::parse(data) {
        std::hint::black_box((datagram.src_port, datagram.dst_port));
    }
}

/// Every target, by name, so the smoke runner and CI iterate one list.
pub const TARGETS: [(&str, fn(&[u8])); 4] = [
    ("nvme_prp", nvme_prp),
    ("virtqueue_chain", virtqueue_chain),
    ("xhci_trb", xhci_trb),
    ("nat_packet", nat_packet),
];
