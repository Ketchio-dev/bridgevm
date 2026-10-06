import ApplicationServices
import Foundation

enum T17ChooserAXFactory {
    static func make(pid: pid_t, button: String, target: AXUIElement,
                     deadline: TimeInterval, now: @escaping () -> TimeInterval) throws -> T17FileChooserDriving {
        let io = T17FileChooserAXIO(budget: T17ChooserNativeBudget(deadline: deadline, now: now))
        return T17FileChooserAX(pid: pid, selection: try T17ChooserSelectionTarget.resolve(button: button), io: io) {
            try T17ChooserOpenAction.perform(identifier: button, deadline: deadline,
                enabled: { (try io.attribute(target, kAXEnabledAttribute) as? NSNumber)?.boolValue },
                press: { AXUIElementPerformAction(target, kAXPressAction as CFString) }, now: now)
        }
    }
}
