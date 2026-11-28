import Foundation

enum FirstRunImportStage: Equatable, Sendable {
    case validating, copying
    /// A container disk becoming the bundle's RAW disk, with the percent converted.
    case converting(HvfImportedDiskFormat, percent: Int)
    case saving, readingLibrary, complete

    /// Stages only move forward, and a conversion's percent only rises.
    var order: (Int, Int) {
        switch self {
        case .validating: return (0, 0)
        case .copying: return (1, 0)
        case .converting(_, let percent): return (1, percent)
        case .saving: return (2, 0)
        case .readingLibrary: return (3, 0)
        case .complete: return (4, 0)
        }
    }

    /// Nil (the spinner) until the converter reports its first percent: a fixed VHD's copy reports none, and
    /// its bar sat at 0% with nothing moving for as long as the copy took.
    var conversionFraction: Double? {
        guard case let .converting(_, percent) = self, percent > 0 else { return nil }
        return Double(percent) / 100
    }

    var label: String {
        switch self {
        case .validating: return "Checking input files…"
        case .copying: return "Copying VM files…"
        case .converting(let format, let percent): return format.conversionLabel(percent: percent)
        case .saving: return "Saving the VM to your library…"
        case .readingLibrary: return "Checking your library…"
        case .complete: return "The VM is ready in your library."
        }
    }
}

enum FirstRunImportResult: Sendable {
    case committed(VMConfig)
    case failed(String)
    case publishedButUnsynced(VMConfig, String)
}

typealias FirstRunImportOperation = @Sendable (FirstRunImport.Inputs, URL, URL,
    @escaping @Sendable (FirstRunImportStage) -> Void) -> FirstRunImportResult

struct FirstRunImportProgress {
    private(set) var operationID: UUID?
    private(set) var displayName: String?
    private(set) var stage: FirstRunImportStage?
    private(set) var error: String?
    private(set) var publishedConfig: VMConfig?
    private(set) var savedWithNotice = false
    private var publicationWarning: String?
    var isBusy: Bool { operationID != nil }

    mutating func begin(displayName: String? = nil) -> UUID {
        self.displayName = displayName
        error = nil
        stage = .validating
        let id = UUID()
        operationID = id
        return id
    }

    mutating func beginRecovery() {
        displayName = publishedConfig?.name
        operationID = UUID()
        stage = .readingLibrary
        // Keep the publication warning visible until explicit read-back succeeds.
    }

    mutating func advance(_ next: FirstRunImportStage, operation: UUID) {
        guard operationID == operation, stage.map({ next.order >= $0.order }) ?? true else { return }
        stage = next
    }

    mutating func finish(error: String?, publishedConfig: VMConfig? = nil, savedWithNotice: Bool = false) {
        operationID = nil
        self.error = error
        self.publishedConfig = publishedConfig
        self.savedWithNotice = savedWithNotice
        publicationWarning = publishedConfig == nil ? nil : error
        if error == nil || savedWithNotice { stage = .complete }
    }

    mutating func recoveryFailed(_ message: String) {
        operationID = nil
        savedWithNotice = false
        error = [publicationWarning, message].compactMap { $0 }.joined(separator: "\n")
    }

    mutating func returnToInputs() { self = FirstRunImportProgress() }
}
