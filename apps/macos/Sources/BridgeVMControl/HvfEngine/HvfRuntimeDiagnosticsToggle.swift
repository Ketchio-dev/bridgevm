import SwiftUI

/// SwiftUI leaves collapsed diagnostics out of the accessibility tree, so the
/// label is a stable button that automation can press to open them.
struct HvfRuntimeDiagnosticsToggle: View {
    @Binding var expanded: Bool

    var body: some View {
        Button("Advanced diagnostics") { expanded.toggle() }
            .buttonStyle(.plain)
            .accessibilityIdentifier("bridgevm.runtime.diagnostics.toggle")
    }
}
