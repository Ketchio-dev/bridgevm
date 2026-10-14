import SwiftUI

struct RetainedControlDetailView: View {
    @ObservedObject var library: LibraryModel
    let record: LibraryRetainedControlRecord

    var body: some View {
        record.descriptor.detail { library.dismissRetainedControl(record.id) }
    }
}
