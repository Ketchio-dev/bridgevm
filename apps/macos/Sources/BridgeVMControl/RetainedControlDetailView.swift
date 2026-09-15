import SwiftUI

struct RetainedControlDetailView: View {
    @ObservedObject var library: LibraryModel
    let record: LibraryRetainedControlRecord

    var body: some View {
        switch record.descriptor {
        case let .runtime(config, session):
            RetainedRuntimeControlView(config: config, session: session) {
                library.dismissRetainedControl(record.id)
            }
        case let .install(config, session):
            RetainedInstallControlView(config: config, session: session) {
                library.dismissRetainedControl(record.id)
            }
        }
    }
}
