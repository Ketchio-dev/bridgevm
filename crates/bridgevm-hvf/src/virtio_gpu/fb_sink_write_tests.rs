use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn sink() -> FbSink {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    FbSink::at_path(std::env::temp_dir().join(format!(
        "bridgevm-fb-sink-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )))
}

fn pixels(sink: &FbSink, len: usize) -> &[u8] {
    unsafe { std::slice::from_raw_parts(sink.map.add(HEADER_LEN), len) }
}

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn first_damaged_publish_initializes_the_complete_frame() {
    let mut sink = sink();
    let frame: Vec<u8> = (0..48).collect();
    sink.write_damage(4, 3, 16, 7, &frame, rect(1, 1, 1, 1));
    assert_eq!(pixels(&sink, 48), frame);
    assert_eq!(sink.lock_shared().seq, 2);
    assert_eq!(unsafe { read_u32(sink.map, 8) }, 4);
}

#[test]
fn subsequent_damage_changes_only_the_clipped_rows() {
    let mut sink = sink();
    let original = vec![1; 48];
    sink.write(4, 3, 16, 7, &original);
    let changed = vec![9; 48];
    sink.write_damage(4, 3, 16, 7, &changed, rect(3, 2, 10, 10));
    let mut expected = original;
    expected[44..48].fill(9);
    assert_eq!(pixels(&sink, 48), expected);
    assert_eq!(sink.lock_shared().seq, 4);
}

#[test]
fn empty_damage_preserves_pixels_and_publishes_an_even_sequence() {
    let mut sink = sink();
    sink.write(2, 2, 8, 7, &[3; 16]);
    sink.write_damage(2, 2, 8, 7, &[8; 16], rect(8, 8, 2, 2));
    assert_eq!(pixels(&sink, 16), &[3; 16]);
    assert_eq!(sink.lock_shared().seq, 4);
    let published = sink.sequence().load(Ordering::Acquire);
    assert_eq!(published, 4);
    assert_eq!(published & 1, 0);
}

#[test]
fn layout_change_forces_a_complete_frame_even_with_damage() {
    let mut sink = sink();
    sink.write(4, 2, 16, 7, &[1; 32]);
    sink.write_damage(2, 2, 8, 7, &[6; 16], rect(0, 0, 1, 1));
    assert_eq!(pixels(&sink, 16), &[6; 16]);
    assert_eq!(unsafe { read_u32(sink.map, 8) }, 2);
    assert_eq!(unsafe { read_u32(sink.map, 16) }, 8);
}

#[test]
fn short_source_is_rejected_before_mapping_or_sequence_change() {
    let mut sink = sink();
    sink.write_damage(4, 3, 16, 7, &[0; 47], rect(0, 0, 4, 3));
    assert!(sink.map.is_null());
    assert_eq!(sink.lock_shared().seq, 0);
}

#[test]
fn shared_writers_take_turns_on_one_sequence() {
    let device = sink();
    let ramfb = device.share();
    let mut writers = [device, ramfb];
    for frame in 1..=4u8 {
        let writer = &mut writers[usize::from(frame % 2)];
        writer.write(2, 1, 8, 7, &[frame; 8]);
        let published = writer.sequence().load(Ordering::Acquire);
        assert_eq!(published, u64::from(frame) * 2, "frame {frame}");
        assert_eq!(pixels(writer, 8), &[frame; 8]);
    }
    assert_eq!(writers[0].sequence().load(Ordering::Acquire), 8);
    assert_eq!(writers[1].lock_shared().seq, 8);
}

#[test]
fn damage_after_another_writers_frame_is_published_whole() {
    let mut device = sink();
    let mut ramfb = device.share();
    device.write(4, 3, 16, 7, &[1; 48]);
    ramfb.write(4, 3, 16, 7, &[2; 48]);
    device.write_damage(4, 3, 16, 7, &[3; 48], rect(0, 0, 1, 1));
    assert_eq!(
        pixels(&device, 48),
        &[3; 48],
        "none of the other frame remains"
    );
    device.write_damage(4, 3, 16, 7, &[4; 48], rect(0, 0, 1, 1));
    let mut expected = vec![3; 48];
    expected[..4].fill(4);
    assert_eq!(
        pixels(&device, 48),
        expected,
        "on its own frame, damage only"
    );
    assert_eq!(ramfb.lock_shared().seq, 8);
}
