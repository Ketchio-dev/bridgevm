import SwiftUI

extension Scene {
    @MainActor
    func controlWindowPresentation() -> some Scene {
        windowStyle(.titleBar)
            .defaultSize(width: 1320, height: 860)
    }
}
