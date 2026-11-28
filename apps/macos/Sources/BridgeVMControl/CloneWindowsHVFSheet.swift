import SwiftUI

struct CloneWindowsHVFSheet: View {
    @ObservedObject var library: LibraryModel
    let config: VMConfig
    @Environment(\.dismiss) private var dismiss
    @State private var name: String

    init(library: LibraryModel, config: VMConfig) {
        self.library = library
        self.config = config
        _name = State(initialValue: Self.suggestedName(for: config.name))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("Duplicate Virtual Machine").font(.title2.bold())
            HStack(spacing: 12) {
                LibraryVMEmblem(engine: config.engineKind, compact: true)
                VStack(alignment: .leading, spacing: 3) {
                    Text("Original VM").font(.caption).foregroundStyle(.secondary)
                    Text(config.name).font(.headline).lineLimit(2).truncationMode(.middle)
                        .help(config.name)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("bridgevm.clone.source")
            }
            VStack(alignment: .leading, spacing: 6) {
                Text("Name for the duplicate").font(.callout.weight(.medium))
                TextField("Name for the duplicate", text: $name)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("bridgevm.clone.name")
                if VMLibrary.normalizedVMName(name) == nil {
                    Text(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                         ? "Enter a name for the duplicate." : "Use a shorter name and remove any line breaks or control characters.")
                        .font(.caption).foregroundStyle(.red)
                }
            }
            Text("The duplicate gets its own disk, boot settings and copies of any snapshots, and a new TPM. When it first starts, sign in to Windows with your password, because your PIN won’t work there, and if BitLocker or device encryption is on, have the recovery key ready. The original VM doesn’t change.")
                .font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            Divider()
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Duplicate") {
                    library.cloneWindowsHVF(config, name: name)
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(VMLibrary.normalizedVMName(name) == nil)
                .accessibilityIdentifier("bridgevm.clone.confirm")
            }
        }
        .padding(22)
        .frame(width: 480, alignment: .leading)
    }

    private static func suggestedName(for source: String) -> String {
        let suffix = " Copy"
        var prefix = String(source.prefix(VMLibrary.maximumVMNameCharacters - suffix.count))
        while !prefix.isEmpty {
            if let name = VMLibrary.normalizedVMName(prefix + suffix) { return name }
            prefix.removeLast()
        }
        return "Windows Copy"
    }
}
