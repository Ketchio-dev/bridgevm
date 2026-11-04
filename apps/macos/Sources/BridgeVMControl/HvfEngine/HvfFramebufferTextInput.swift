#if canImport(AppKit)
import AppKit

/// The local IME owns marked text; only its committed result enters the guest queue.
/// The small preedit view also gives the candidate window a truthful host anchor.
@MainActor
class HvfFramebufferTextInput: NSView, NSTextInputClient {
    private var composition = HvfTextCompositionState()
    private var interpretingEvent: NSEvent?
    private var interpretingTarget: HvfTextInputTargetIdentity?
    private var interpretationRefused = false
    private var discardingComposition = false
    private var preeditView: HvfTextCompositionOverlay?
    private var compositionTarget: HvfTextInputTargetIdentity?
    private var modifierTap = HvfModifierTap()

    var guestInputAvailable: Bool { false }
    var guestInputIdentity: HvfTextInputTargetIdentity? { nil }
    /// Whether the guest takes generic chords now (HvfHostKeyMapping.modifierChords).
    var guestModifierChords: Bool { false }
    func sendGuestKeyEvent(_ event: NSEvent) -> Bool { false }
    func sendGuestModifierTap(_ action: String) {}
    func sendGuestText(_ text: String) -> Bool { false }
    func textInputRefused() { NSSound.beep() }
    /// A key, click, other modifier or focus loss ends a pending bare Command or Option tap.
    func cancelModifierTap() { modifierTap.cancel() }

    /// With generic chords, Command chords reach the guest before the app menu sees them; the app
    /// shortcuts the Mac keeps (HvfHostKeyCommand.isReservedForMac) resolve to nothing and stay with it.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard event.type == .keyDown, event.modifierFlags.contains(.command), window?.firstResponder === self,
              guestInputAvailable, guestModifierChords else { return super.performKeyEquivalent(with: event) }
        cancelModifierTap()
        _ = synchronizeTextInputTarget()
        commitTextComposition()
        return sendGuestKeyEvent(event) || super.performKeyEquivalent(with: event)
    }

    override func flagsChanged(with event: NSEvent) {
        if guestInputAvailable, guestModifierChords,
           let action = modifierTap.changed(keyCode: event.keyCode, flags: event.modifierFlags) {
            _ = synchronizeTextInputTarget()
            commitTextComposition()
            sendGuestModifierTap(action)
        } else if !guestInputAvailable || !guestModifierChords {
            cancelModifierTap()
        }
        super.flagsChanged(with: event)
    }

    override func keyDown(with event: NSEvent) {
        cancelModifierTap()
        guard guestInputAvailable else { super.keyDown(with: event); return }
        _ = synchronizeTextInputTarget()
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        // Keep existing host shortcuts and guest physical chords. Text, Option dead keys and
        // navigation during preedit must first be offered to the input method, unless the guest
        // takes generic chords: then Option is Alt for every key.
        let chordFlags: NSEvent.ModifierFlags = guestModifierChords ? [.command, .control, .option] : [.command, .control]
        if !flags.intersection(chordFlags).isEmpty {
            commitTextComposition()
            if !sendGuestKeyEvent(event) { super.keyDown(with: event) }
            return
        }
        if !hasMarkedText(), HvfTextCompositionKeyRouting.isPhysical(event) {
            if !sendGuestKeyEvent(event) { super.keyDown(with: event) }
            return
        }
        interpretingEvent = event
        interpretingTarget = guestInputIdentity
        interpretationRefused = false
        defer { interpretingEvent = nil; interpretingTarget = nil; interpretationRefused = false }
        interpretKeyEvents([event])
    }

    override func doCommand(by selector: Selector) {
        guard guestInputAvailable, synchronizeTextInputTarget() else { return }
        if selector == #selector(NSResponder.cancelOperation(_:)), hasMarkedText() {
            cancelTextComposition()
            return
        }
        if let event = interpretingEvent {
            if hasMarkedText() { unmarkText() }
            guard synchronizeTextInputTarget() else { return }
            if sendGuestKeyEvent(event) { return }
        }
        super.doCommand(by: selector)
    }

    func setMarkedText(_ value: Any, selectedRange: NSRange, replacementRange: NSRange) {
        guard guestInputAvailable, !discardingComposition, synchronizeTextInputTarget() else { return }
        guard let text = Self.string(value),
              composition.mark(text, selectedRange: selectedRange, replacementRange: replacementRange) else {
            interpretationRefused = true; textInputRefused(); return
        }
        compositionTarget = guestInputIdentity
        refreshPreedit()
    }

    func insertText(_ value: Any, replacementRange: NSRange) {
        guard guestInputAvailable, !discardingComposition, synchronizeTextInputTarget() else { return }
        guard let text = Self.string(value),
              let committed = composition.replacing(with: text, range: replacementRange) else {
            interpretationRefused = true; textInputRefused(); return
        }
        composition.clear()
        compositionTarget = nil
        refreshPreedit()
        if !committed.isEmpty, !sendGuestText(committed) { interpretationRefused = true; textInputRefused() }
    }

    func unmarkText() {
        guard hasMarkedText(), !discardingComposition else { return }
        // NSTextInputClient defines unmark as accepting pending text, not cancellation.
        insertText(composition.text, replacementRange: NSRange(location: NSNotFound, length: 0))
    }

    func cancelTextComposition() {
        guard !discardingComposition else { return }
        discardingComposition = true
        composition.clear()
        compositionTarget = nil
        refreshPreedit()
        inputContext?.discardMarkedText()
        discardingComposition = false
    }

    /// Commits the syllable being composed, as a Mac does before a shortcut, so it reaches the guest
    /// before whatever follows. The input method is reset under `discardingComposition`, so it cannot
    /// insert the text a second time. `admits` can refuse the text, which is then discarded.
    func commitTextComposition(admits: (String) -> Bool = { _ in true }) {
        let text = composition.text
        cancelTextComposition()
        guard !text.isEmpty, admits(text) else { return }
        if !sendGuestText(text) { textInputRefused() }
    }

    @discardableResult
    func synchronizeTextInputTarget() -> Bool {
        let eventChanged = interpretingEvent != nil && (interpretingTarget != guestInputIdentity || interpretationRefused)
        guard eventChanged || (hasMarkedText() && compositionTarget != guestInputIdentity) else { return true }
        cancelTextComposition()
        return false
    }

    override func resignFirstResponder() -> Bool {
        guard super.resignFirstResponder() else { return false }
        cancelModifierTap()
        cancelTextComposition()
        return true
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        cancelTextComposition()
    }

    func hasMarkedText() -> Bool { !composition.text.isEmpty }
    func markedRange() -> NSRange { composition.range }
    func selectedRange() -> NSRange { composition.selection }
    func validAttributesForMarkedText() -> [NSAttributedString.Key] { [] }

    func attributedSubstring(forProposedRange range: NSRange, actualRange: NSRangePointer?) -> NSAttributedString? {
        actualRange?.pointee = NSRange(location: NSNotFound, length: 0)
        guard hasMarkedText(), range.location != NSNotFound, range.location >= 0, range.length >= 0,
              range.location <= composition.text.utf16.count else { return nil }
        let clipped = NSRange(location: range.location,
            length: min(range.length, composition.text.utf16.count - range.location))
        guard let indices = HvfTextCompositionState.indices(clipped, in: composition.text) else { return nil }
        actualRange?.pointee = clipped
        return NSAttributedString(string: String(composition.text[indices]))
    }

    func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
        layoutSubtreeIfNeeded()
        let anchor = preeditView?.frame ?? NSRect(x: 12, y: isFlipped ? max(0, bounds.height - 36) : 12, width: 1, height: 24)
        let geometry = HvfTextCompositionGeometry.rect(text: composition.text, requested: range,
            anchor: anchor, font: preeditView?.font ?? .systemFont(ofSize: 18))
        actualRange?.pointee = geometry.range
        guard let window else { return .zero }
        return window.convertToScreen(convert(geometry.rect, to: nil))
    }

    func characterIndex(for point: NSPoint) -> Int { NSNotFound }

    private func refreshPreedit() {
        if hasMarkedText(), preeditView == nil {
            let view = HvfTextCompositionOverlay.make()
            addSubview(view)
            NSLayoutConstraint.activate([
                view.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
                view.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -12),
                view.widthAnchor.constraint(lessThanOrEqualTo: widthAnchor, constant: -24)
            ])
            preeditView = view
        }
        preeditView?.stringValue = composition.text
        preeditView?.isHidden = !hasMarkedText()
        inputContext?.invalidateCharacterCoordinates()
    }

    private static func string(_ value: Any) -> String? {
        (value as? NSAttributedString)?.string ?? value as? String
    }
}

#endif
