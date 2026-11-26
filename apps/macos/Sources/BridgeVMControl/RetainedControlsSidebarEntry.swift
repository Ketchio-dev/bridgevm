import SwiftUI

struct RetainedControlsSidebarEntry: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        if !library.retainedControlRecords.isEmpty {
            Section("Recent operations") {
                ForEach(library.retainedControlRecords) { record in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(record.descriptor.config.name)
                        Text("\(record.descriptor.kindLabel) · \(record.descriptor.statusLabel)")
                            .font(.caption).foregroundColor(.secondary)
                    }
                    .tag(record.id)
                    .help("This operation was kept after the VM left the library.")
                }
            }
        }
    }
}
