import SwiftUI

extension Scene {
    @MainActor
    func controlWindowPresentation() -> some Scene {
        let scene = windowStyle(.titleBar)
            .defaultSize(width: 1320, height: 860)
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        return scene.defaultLaunchBehavior(.presented)
        #else
        return scene
        #endif
    }
}
