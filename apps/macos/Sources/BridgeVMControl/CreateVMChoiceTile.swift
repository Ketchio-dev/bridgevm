import SwiftUI

struct CreateVMChoiceTile: View {
    let title: String
    let icon: String
    let selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 8) {
                Image(systemName: icon).font(.system(size: 28))
                HStack(spacing: 4) {
                    Text(title).font(.callout)
                    Image(systemName: "checkmark")
                        .font(.caption.bold())
                        .opacity(selected ? 1 : 0)
                        .accessibilityHidden(true)
                }
            }
            .frame(maxWidth: .infinity).padding(.vertical, 16)
            .background(selected ? Color.accentColor.opacity(0.18) : Color.gray.opacity(0.1))
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(selected ? Color.accentColor : .clear, lineWidth: 2))
            .cornerRadius(10)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(title)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

struct CreateVMMethodChoiceTile: View {
    let title: String
    let selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Text(title)
                    .font(.caption)
                Image(systemName: "checkmark")
                    .font(.caption2.bold())
                    .opacity(selected ? 1 : 0)
                    .accessibilityHidden(true)
            }
            .padding(.horizontal, 12).padding(.vertical, 7)
            .frame(maxWidth: .infinity)
            .background(selected ? Color.accentColor.opacity(0.18) : Color.gray.opacity(0.08))
            .overlay(RoundedRectangle(cornerRadius: 8).stroke(selected ? Color.accentColor : .clear, lineWidth: 1.5))
            .cornerRadius(8)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(title)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}
