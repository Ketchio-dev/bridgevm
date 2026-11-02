import SwiftUI

extension LibraryDashboardRuntimeStopControl {
    var presentation: HvfRuntimeStopPresentation {
        .make(runtimeActive: runtimeActive, lifecycleBusy: lifecycleBusy,
              ownsRuntime: session.ownedProcessIdentity != nil)
    }
}
