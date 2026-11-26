import SwiftUI

struct LibraryWindowPresentation {
    var sheet: Sheet?
    var cloneSource: VMConfig?
    var preparingDisk = false

    enum Sheet: Hashable, Identifiable {
        case palette
        case create(installer: URL?)

        var id: Self { self }
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
