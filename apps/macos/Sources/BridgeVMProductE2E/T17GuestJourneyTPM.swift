import Foundation

extension T17GuestJourney {
    /// Records this boot's guest vTPM observation in the lane's log. It neither throws
    /// nor proves a stage, so the journey continues whatever the guest reports.
    func readTPM(_ boot: T17TPMBoot) {
        let reader = T17TPMBootReader(
            share: URL(fileURLWithPath: request.sharePath, isDirectory: true),
            runLog: URL(fileURLWithPath: request.bundlePath).appendingPathComponent("logs/hvf/run.log"),
            nonce: request.nonce, timeouts: tpm.timeouts)
        tpm.record(reader.read(boot) { try T17RuntimeControlInput.enter($0, ui: ui) })
    }
}
