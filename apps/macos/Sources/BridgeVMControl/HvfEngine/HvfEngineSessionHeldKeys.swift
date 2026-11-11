import Foundation

/// The Mac keys held down on the guest, by key code: a named key or chord goes down at the Mac's key-down and up at
/// its key-up (HvfModifierChord.down), at most six at once like a boot keyboard. Where each went down is the router's
/// (HvfSessionInputRouter.routeKeyUp); this only remembers which action a key code pressed, since the modifiers may
/// have changed by its key-up.
struct HvfHeldKeys {
    static let maximum = 6
    private(set) var actions: [(keyCode: UInt16, action: String)] = []

    func action(_ keyCode: UInt16) -> String? { actions.first { $0.keyCode == keyCode }?.action }

    mutating func press(_ keyCode: UInt16, _ action: String) { actions.append((keyCode, action)) }

    mutating func release(_ keyCode: UInt16) -> String? {
        guard let index = actions.firstIndex(where: { $0.keyCode == keyCode }) else { return nil }
        return actions.remove(at: index).action
    }
}

extension HvfEngineSession {
    /// A key-down that resolved to `action` (HvfFramebufferKeyInput). It holds the key where it can: on input.ctl the
    /// VMM keeps it in its HID report and Windows repeats it, so a Mac repeat sends nothing; on the ordered route
    /// SendInput has no typematic, so a Mac repeat is one more key-down, as Windows' own would be, never an extra
    /// key-up. Where it cannot (an agent without the HOLD marker, a pending paste, a Ctrl+Alt+Delete still going in,
    /// six keys already down) every press and repeat is a whole tap, as before.
    func sendKeyDown(_ action: String, keyCode: UInt16, repeat isRepeat: Bool) -> Bool {
        if let current = heldKeys.action(keyCode) {
            guard current != action else { return repeatHeldKey(action) }
            sendKeyUp(keyCode: keyCode) // a modifier changed under the held key
        }
        guard legacyPaste.pending == nil, heldKeys.actions.count < HvfHeldKeys.maximum, inputDriver.router.keyHold,
              !HvfGuestInputEncoding.isSecureAttention(action), HvfModifierChord.eventCount(action) != nil else {
            return HvfDisplayKeyRepeat.send(action, repeat: isRepeat, to: self)
        }
        guard sendKey(HvfModifierChord.down + action) else { return false }
        heldKeys.press(keyCode, action)
        return true
    }

    /// The Mac's key-up: the key goes up on the route it went down. Never refused: a paste, a pause or a Ctrl+Alt+Delete
    /// cannot hold back a release.
    func sendKeyUp(keyCode: UInt16) {
        guard let action = heldKeys.release(keyCode) else { return }
        releaseHeldKey(action)
    }

    /// Every held key goes up now, on its own route: before Ctrl+Alt+Delete, when a Command key comes down (the Mac
    /// sends no key-up for a key released while Command is down), when the menu bar opens.
    func releaseHeldKeys() {
        while let held = heldKeys.actions.first { _ = heldKeys.release(held.keyCode); releaseHeldKey(held.action) }
    }

    /// A target change (focus loss, Pause, Stop): the keys input.ctl holds go up there now; those the ordered route
    /// pressed go up through the cleanup that cancelling its target starts (HvfPointerCleanup).
    func releaseHeldKeysForTargetChange() {
        heldKeys = HvfHeldKeys()
        for action in inputDriver.router.legacyHeldKeys { appendLiveInput("KEY " + HvfModifierChord.up + action) }
    }

    private func repeatHeldKey(_ action: String) -> Bool {
        if inputDriver.router.legacyHeldKeys.contains(action) { return true }
        guard admitsDisplayKeyRepeat else { return true }
        return sendKey(HvfModifierChord.down + action)
    }

    private func releaseHeldKey(_ action: String) {
        if inputDriver.routeKeyUp(action, binding: inputBinding) == .legacy { appendLiveInput("KEY " + HvfModifierChord.up + action) }
    }
}
