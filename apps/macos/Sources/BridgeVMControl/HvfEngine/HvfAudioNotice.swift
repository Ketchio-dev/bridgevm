import SwiftUI

/// Sound or the microphone the VMM could not start on the Mac. The VM runs on without them and
/// prints one line to the run's log (hda_coreaudio.rs start_output, hda_microphone.rs supervise);
/// until this notice nothing else said so. Cleared at each run, and the microphone's once it opens.
@MainActor
final class HvfAudioNotice: ObservableObject {
    static let outputOffPrefix = "HDA CoreAudio output: off for this run"
    static let microphoneOffPrefix = "HDA capture: the Mac microphone did not start"
    static let microphoneOnPrefix = "HDA capture: the Mac microphone is on again"
    @Published private(set) var soundOff = false
    @Published private(set) var microphoneOff = false

    func consume(_ line: String) {
        if line.hasPrefix(Self.outputOffPrefix), !soundOff { soundOff = true }
        if line.hasPrefix(Self.microphoneOffPrefix), !microphoneOff { microphoneOff = true }
        if line.hasPrefix(Self.microphoneOnPrefix), microphoneOff { microphoneOff = false }
    }

    func reset() {
        if soundOff { soundOff = false }
        if microphoneOff { microphoneOff = false }
    }

    var text: String? {
        if soundOff {
            return "Windows has no sound this time because the Mac’s audio output didn’t start. Restart the VM to try again."
        }
        if microphoneOff {
            return "The Mac microphone didn’t start, so Windows recorded silence. Windows tries again the next time an app starts recording."
        }
        return nil
    }
}

struct HvfAudioNoticeView: View {
    @ObservedObject var notice: HvfAudioNotice

    var body: some View {
        if let text = notice.text {
            Label(text, systemImage: "speaker.slash").font(.caption).foregroundStyle(.secondary)
                .accessibilityIdentifier("bridgevm.runtime.audio.notice")
        }
    }
}
