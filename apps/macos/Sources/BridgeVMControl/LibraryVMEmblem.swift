import SwiftUI

// An abstract engine emblem, never a guest screenshot or a runtime indicator.
struct LibraryVMEmblem: View {
    let engine: BackendKind
    var compact = false

    private var color: Color { engine == .hvfEngine ? .blue : engine == .fastVZ ? .teal : .purple }

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: compact ? 10 : 14)
                .fill(color.gradient.opacity(0.15))
            if engine == .hvfEngine {
                BridgeVMMark().foregroundStyle(color)
                    .frame(width: compact ? 27 : 48, height: compact ? 27 : 48)
            } else {
                Image(systemName: "desktopcomputer")
                    .font(.system(size: compact ? 23 : 42, weight: .light))
                    .foregroundStyle(color)
            }
        }
        .frame(width: compact ? 46 : 88, height: compact ? 46 : 76)
        .accessibilityHidden(true)
    }
}
