//! Starting the CoreAudio sink. When the Mac's audio output cannot start, sound
//! is off for this run and one plain line says so; the VM keeps running, and
//! the guest's HDA device stays attached without a host sink.

/// Printed once the sink is installed.
pub(crate) const ENABLED: &str = "HDA CoreAudio output: s16le 48000 Hz stereo, enabled";

/// Open the sink and install it. On failure nothing is installed and the
/// error is the one line to print instead.
pub(crate) fn start_output<S>(
    open: impl FnOnce() -> Result<S, String>,
    install: impl FnOnce(S),
) -> Result<(), String> {
    let sink = open().map_err(|error| {
        format!(
            "HDA CoreAudio output: off for this run, the Mac's audio output did not start \
             ({error}); Windows runs without sound, restart the VM to try again"
        )
    })?;
    install(sink);
    Ok(())
}

#[cfg(test)]
#[path = "hda_coreaudio_start_tests.rs"]
mod tests;
