//! The teardown records B7, T17 and A5 parse print byte-identically. The
//! diagnostic records join them only through the HDA controller's drop, before
//! this sink drops: the sink's teardown prints exactly what it printed before.

const SINK: &str = include_str!("hda_coreaudio.rs");
const TEARDOWN: &str = include_str!("hda_coreaudio_teardown.rs");

#[test]
fn the_lifecycle_stats_and_enqueue_records_print_byte_identically() {
    for literal in [
        "\"hda CoreAudio lifecycle: operation={operation} osstatus={status} success={}\"",
        "\"hda CoreAudio stats: frames_rendered={frames} drops={drops} dropped_bytes={dropped_bytes} \
         format_drops={format_drops} ring_full_drops={ring_full_drops} queue_stop_errors={} \
         queue_dispose_errors={} callback_errors={} callback_active_errors={} \
         callback_stopping_errors={} callback_expected_stopping_errors={} \
         callback_unexpected_errors={} callback_stopping_invalid_run_state={} \
         callback_stopping_queue_invalidated={} callback_stopping_enqueue_during_reset={} \
         callback_stopping_disposal_pending={} callback_stopping_unclassified={}\"",
        "\"hda CoreAudio callback enqueue: state=active reason={} osstatus={} expected=false\"",
        "\"hda CoreAudio callback enqueue: state=stopping reason={} osstatus={} expected={}\"",
    ] {
        assert_eq!(TEARDOWN.matches(literal).count(), 1, "{literal}");
    }
}

#[test]
fn the_sink_prints_the_stats_then_the_continuity_record_and_nothing_more() {
    let sink: Vec<&str> = SINK.lines().map(str::trim).collect();
    let stats = sink
        .iter()
        .position(|line| *line == "self.shared.print_stats([stop_status, dispose_status]);")
        .expect("teardown prints the stats record");
    let continuity = "println!(\"{}\", self.shared.continuity.snapshot().record());";
    assert_eq!(sink[stats + 1..stats + 3], [continuity, "}"]);
    assert_eq!(SINK.matches("println!").count(), 1);
}
