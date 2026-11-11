use super::*;

fn counted(presses: &Cell<u32>, works: bool) -> OwnerEof<impl Fn() -> bool + '_> {
    OwnerEof::new(
        move || {
            presses.set(presses.get() + 1);
            works
        },
        BUDGET,
    )
}

#[test]
fn owner_eof_presses_the_power_button_before_any_stop() {
    let presses = Cell::new(0);
    let eof = counted(&presses, true);
    let start = Instant::now();
    assert!(!eof.cancel(None, false, true, start));
    assert_eq!(presses.get(), 0);
    assert!(
        !eof.cancel(Some("ownerEOF"), false, true, start),
        "Windows gets the shutdown request, not the helper a TERM"
    );
    assert!(!eof.cancel(Some("ownerEOF"), false, true, start + BUDGET / 2));
    assert_eq!(presses.get(), 1, "one press per run");
    assert!(
        eof.cancel(Some("ownerEOF"), false, false, start + BUDGET / 2),
        "the guest turned off: the run ends without restarting it"
    );
}

#[test]
fn the_budget_falls_back_to_the_stop() {
    let presses = Cell::new(0);
    let eof = counted(&presses, true);
    let start = Instant::now();
    assert!(!eof.cancel(Some("ownerEOF"), false, true, start));
    let almost = start + BUDGET - Duration::from_millis(1);
    assert!(!eof.cancel(Some("ownerEOF"), false, true, almost));
    assert!(eof.cancel(Some("ownerEOF"), false, true, start + BUDGET));
    assert_eq!(presses.get(), 1);
}

#[test]
fn a_second_signal_falls_back_at_once() {
    let presses = Cell::new(0);
    let eof = counted(&presses, true);
    let start = Instant::now();
    assert!(!eof.cancel(Some("ownerEOF"), false, true, start));
    assert!(eof.cancel(Some("ownerEOF"), true, true, start));
    let early = counted(&presses, true);
    assert!(early.cancel(Some("ownerEOF"), true, true, start));
    assert_eq!(presses.get(), 1, "a TERM before the press skips it");
}

#[test]
fn without_a_guest_or_a_press_owner_eof_stops_at_once() {
    let presses = Cell::new(0);
    let now = Instant::now();
    let eof = counted(&presses, true);
    assert!(
        eof.cancel(Some("ownerEOF"), false, false, now),
        "no helper yet: nothing may start"
    );
    assert_eq!(presses.get(), 0);
    assert!(counted(&presses, false).cancel(Some("ownerEOF"), false, true, now));
    for cause in ["stopRequested", "signal"] {
        assert!(eof.cancel(Some(cause), false, true, now));
    }
}

#[test]
fn an_owner_whose_output_closed_first_also_gets_the_shutdown() {
    // A crash closes both pipes; a failed write to the owner may latch protocolFailed before its EOF.
    let presses = Cell::new(0);
    let eof = counted(&presses, true);
    let start = Instant::now();
    assert!(!eof.cancel(Some("protocolFailed"), false, true, start));
    assert_eq!(presses.get(), 1);
    assert!(!eof.cancel(Some("protocolFailed"), false, true, start + BUDGET / 2));
    assert!(eof.cancel(Some("protocolFailed"), false, true, start + BUDGET));
}

#[test]
fn the_press_appends_one_line_to_the_existing_input_file_only() {
    let dir = std::env::temp_dir().join(format!("bv-owner-eof-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("input.ctl");
    assert!(!press_power_button(&input), "the app creates the file");
    assert!(!input.exists());
    std::fs::write(&input, "KEY a\n").unwrap();
    assert!(press_power_button(&input));
    assert_eq!(
        std::fs::read_to_string(&input).unwrap(),
        "KEY a\nRESUME\nPOWERBUTTON\n",
        "a paused guest is resumed first, so its vCPUs can take the press"
    );
    let link = dir.join("link.ctl");
    std::os::unix::fs::symlink(&input, &link).unwrap();
    assert!(!press_power_button(&link));
    std::fs::remove_dir_all(&dir).unwrap();
}
