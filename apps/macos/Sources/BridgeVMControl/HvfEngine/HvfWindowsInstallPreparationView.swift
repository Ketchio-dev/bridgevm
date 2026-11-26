import SwiftUI

struct HvfWindowsInstallPreparationView: View {
    let config: VMConfig
    @ObservedObject var library: LibraryModel
    @ObservedObject private(set) var preparation: HvfWindowsInstallPreparation

    init(config: VMConfig, library: LibraryModel) {
        self.config = config
        self.library = library
        _preparation = ObservedObject(wrappedValue: library.windowsInstallPreparation(for: config))
    }

    var body: some View {
        switch preparation.state {
        case let .ready(session):
            HvfWindowsInstallView(config: config, library: library, session: session)
        case .preparing:
            VStack(spacing: 12) {
                ProgressView()
                Text("Checking the installation settings…")
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .accessibilityIdentifier("bridgevm.windows.install.preparing")
        case let .failed(message):
            VStack(spacing: 12) {
                Text(message).foregroundColor(.secondary)
                Button("Check Again") { preparation.retry() }
            }
            .padding(20)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .accessibilityIdentifier("bridgevm.windows.install.preparation.failed")
        }
    }
}
