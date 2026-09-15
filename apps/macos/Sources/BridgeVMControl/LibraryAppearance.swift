import SwiftUI
import AppKit

enum LibraryAppearance {
    static let accent = Color.blue
    static let canvas = Color(nsColor: .windowBackgroundColor)
    static let surface = Color(nsColor: .controlBackgroundColor)
    static let inset = Color(nsColor: .textBackgroundColor)
    static let cornerRadius: CGFloat = 16
}

struct LibraryCardSurface: ViewModifier {
    @Environment(\.colorSchemeContrast) private var contrast

    func body(content: Content) -> some View {
        content
            .background(LibraryAppearance.surface, in: RoundedRectangle(cornerRadius: LibraryAppearance.cornerRadius))
            .overlay {
                RoundedRectangle(cornerRadius: LibraryAppearance.cornerRadius)
                    .strokeBorder(.primary.opacity(contrast == .increased ? 0.3 : 0.09), lineWidth: 1)
                    .allowsHitTesting(false)
            }
    }
}

struct LibraryGroupBoxStyle: GroupBoxStyle {
    func makeBody(configuration: Configuration) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            configuration.label.font(.headline)
            configuration.content.frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .modifier(LibraryCardSurface())
    }
}

struct LibraryMetadataLabel: View {
    let title: String
    let value: String
    let symbol: String

    var body: some View {
        Label {
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.caption).foregroundStyle(.secondary)
                Text(value).font(.callout.weight(.semibold))
            }
        } icon: {
            Image(systemName: symbol).font(.title3).foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}
