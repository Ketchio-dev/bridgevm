import Foundation

struct CreateVMDriverSelection {
    enum Status: Equatable { case idle, preparing, prepared, manual, failed(String) }

    private(set) var payloadPath = ""
    private(set) var manifestPath = ""
    private(set) var status: Status = .idle
    private(set) var fetchInFlight = false
    /// The download's percent while it runs; nil while the cached archive is unpacked.
    private(set) var downloadPercent: Int?

    var ready: Bool { status != .preparing && !payloadPath.isEmpty && !manifestPath.isEmpty }

    mutating func choosePayload(_ path: String) {
        payloadPath = path
        status = .manual
    }

    mutating func chooseManifest(_ path: String) {
        manifestPath = path
        status = .manual
    }

    mutating func beginFetch() -> Bool {
        guard !fetchInFlight else { return false }
        fetchInFlight = true
        status = .preparing
        downloadPercent = nil
        return true
    }

    /// Each update hops to the main actor on its own; one that arrives late never moves it back.
    mutating func showDownload(_ percent: Int) {
        guard fetchInFlight else { return }
        downloadPercent = max(percent, downloadPercent ?? 0)
    }

    mutating func finishFetch(_ result: Result<HvfWindowsDriverPayloadFetch.Prepared, Error>) {
        fetchInFlight = false
        downloadPercent = nil
        guard status == .preparing else { return }
        switch result {
        case .success(let prepared):
            payloadPath = prepared.payloadDirectory
            manifestPath = prepared.manifestPath
            status = .prepared
        case .failure(let error):
            let message = (error as? HvfWindowsDriverPayloadFetch.Failure)?.message
                ?? "Couldn’t prepare the drivers. Try again."
            status = .failed(message)
        }
    }
}
