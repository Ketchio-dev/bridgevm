enum WindowsHVFInstallInputPolicy {
    struct StagedInputs {
        let payload: WindowsHVFGuestPayloadPolicy.StagedPayload?
        let unattended: WindowsHVFUnattendPolicy.StagedAnswerFile?
    }

    static func stage(
        payloadDirectory: String?, payloadManifest: String?,
        e2eUnattendedPath: String?, bundlePath: String
    ) -> StagedInputs? {
        let payload: WindowsHVFGuestPayloadPolicy.StagedPayload?
        if payloadDirectory != nil || payloadManifest != nil {
            guard let payloadDirectory, let payloadManifest,
                  let staged = WindowsHVFGuestPayloadPolicy.stage(
                    payloadDirectory: payloadDirectory,
                    manifestPath: payloadManifest, in: bundlePath) else { return nil }
            payload = staged
        } else {
            payload = nil
        }
        let unattended: WindowsHVFUnattendPolicy.StagedAnswerFile?
        if let e2eUnattendedPath {
            guard let staged = WindowsHVFUnattendPolicy.stage(
                e2eUnattendedPath, in: bundlePath) else { return nil }
            unattended = staged
        } else {
            unattended = nil
        }
        return StagedInputs(payload: payload, unattended: unattended)
    }
}
