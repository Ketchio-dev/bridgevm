use super::*;

#[test]
fn an_output_that_cannot_start_turns_sound_off_in_one_line() {
    let mut installed = false;
    let result = start_output(
        || Err::<(), _>("AudioQueueNewOutput failed with OSStatus -50 (0xffffffce)".to_string()),
        |()| installed = true,
    );
    assert!(!installed, "no sink is installed, so the HDA device plays to nothing");
    let line = result.unwrap_err();
    assert_eq!(
        line,
        "HDA CoreAudio output: off for this run, the Mac's audio output did not start \
         (AudioQueueNewOutput failed with OSStatus -50 (0xffffffce)); Windows runs without \
         sound, restart the VM to try again"
    );
    assert!(!line.contains('\n'));
}

#[test]
fn an_output_that_starts_is_installed() {
    let mut installed = None;
    assert_eq!(start_output(|| Ok(7), |sink| installed = Some(sink)), Ok(()));
    assert_eq!(installed, Some(7));
}
