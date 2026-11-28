import SwiftUI

struct LibraryWindowPresentation {
    var sheet: Sheet?
    var cloneSource: VMConfig?
    var deletionTarget: VMConfig?
    var showsImportDraft = false

    enum Sheet: Hashable, Identifiable {
        case palette
        case create(installer: URL?)

        var id: Self { self }
    }
}

/// File > New VM… asks here; the library window opens Create once it is on screen.
@MainActor
final class LibraryWindowRequests: ObservableObject {
    static let shared = LibraryWindowRequests()
    @Published var createRequested = false

    /// Opens Create unless a Create or Duplicate sheet is already open, so a filled-in form stays.
    func take(into presentation: inout LibraryWindowPresentation) {
        guard createRequested else { return }
        createRequested = false
        if case .create = presentation.sheet { return }
        guard presentation.cloneSource == nil else { return }
        presentation.sheet = .create(installer: nil)
    }
}

private struct LibraryWindowPresentationKey: EnvironmentKey {
    static let defaultValue: Binding<LibraryWindowPresentation>? = nil
}

extension EnvironmentValues {
    var libraryWindowPresentation: Binding<LibraryWindowPresentation>? {
        get { self[LibraryWindowPresentationKey.self] }
        set { self[LibraryWindowPresentationKey.self] = newValue }
    }
}
