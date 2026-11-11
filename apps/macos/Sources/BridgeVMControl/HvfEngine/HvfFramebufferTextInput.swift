#if canImport(AppKit)
import AppKit

/// The local IME owns marked text; only its committed result enters the guest queue.
/// The small preedit view also gives the candidate window a truthful host anchor: at the guest's text caret
/// when the guest says where it is (HvfGuestCaret.swift), else at the display's bottom-left.
@MainActor
class HvfFramebufferTextInput: NSView, NSTextInputClient {
    private var composition = HvfTextCompositionState()
    private var interpretingEvent: NSEvent?
    private var interpretingTarget: HvfTextInputTargetIdentity?
    private var interpretationRefused = false
    private var discardingComposition = false
    private var preeditView: HvfTextCompositionOverlay?
    /// The overlay's constraints at the bottom-left, and at a caret (`caretLeft`, `caretTop` set per placement).
    private var bottomLeftPlacement: [NSLayoutConstraint] = []
    private var caretLeft: NSLayoutConstraint?
    private var caretTop: NSLayoutConstraint?
    /// The guest's text caret in view coordinates while text is composed; kept across a syllable committed and
    /// the next one marked in the same key event (Korean), so the overlay does not jump back per syllable.
    private var compositionCaret: NSRect?
    /// Bumped whenever an answer asked for before would be stale: the composition ended, the view resized.
    private var caretEpoch = 0
    private var compositionTarget: HvfTextInputTargetIdentity?
    private var modifierTap = HvfModifierTap()

    var guestInputAvailable: Bool { false }
    var guestInputIdentity: HvfTextInputTargetIdentity? { nil }
    /// Whether the guest takes generic chords now (HvfHostKeyMapping.modifierChords).
    var guestModifierChords: Bool { false }
    /// A key-down, or the key-up of a key held down on the guest (HvfFramebufferKeyInput).
    func sendGuestKeyEvent(_ event: NSEvent) -> Bool { false }
    /// Command came down: the Mac sends no key-up for a key released while it is down, so held keys go up now.
    func releaseGuestHeldKeys() {}
    func sendGuestModifierTap(_ action: String) {}
    func sendGuestText(_ text: String) -> Bool { false }
    func textInputRefused() { NSSound.beep() }
    /// Asks the guest where its text caret is; `found` later gets it in this view's coordinates, or nil for none.
    /// False when not asked (no agent that answers, or a question in flight).
    func askGuestCaret(_ found: @escaping (NSRect?) -> Void) -> Bool { false }
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
        if guestInputAvailable, event.modifierFlags.contains(.command) { releaseGuestHeldKeys() }
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
        // A key that leaves nothing composed may have moved the guest caret (a commit, an arrow key).
        defer { if !hasMarkedText() { forgetCaret() } }
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

    /// Every key-up ends a key held down on the guest; the input method never sees key-ups.
    override func keyUp(with event: NSEvent) {
        if !guestInputAvailable || !sendGuestKeyEvent(event) { super.keyUp(with: event) }
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
        let starting = !hasMarkedText()
        guard let text = Self.string(value),
              composition.mark(text, selectedRange: selectedRange, replacementRange: replacementRange) else {
            interpretationRefused = true; textInputRefused(); return
        }
        compositionTarget = guestInputIdentity
        if !hasMarkedText(), interpretingEvent == nil { forgetCaret() }
        refreshPreedit()
        if starting, hasMarkedText() { askCaret() }
    }

    func insertText(_ value: Any, replacementRange: NSRange) {
        guard guestInputAvailable, !discardingComposition, synchronizeTextInputTarget() else { return }
        guard let text = Self.string(value),
              let committed = composition.replacing(with: text, range: replacementRange) else {
            interpretationRefused = true; textInputRefused(); return
        }
        composition.clear()
        compositionTarget = nil
        // A commit outside a key event (a candidate clicked) ends the caret's use; keyDown judges its own.
        if interpretingEvent == nil { forgetCaret() }
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
        forgetCaret()
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

    /// The caret was mapped for the old size (and the guest display follows the window).
    override func setFrameSize(_ newSize: NSSize) {
        let resized = newSize != frame.size
        super.setFrameSize(newSize)
        if resized { forgetCaret() }
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
            view.widthAnchor.constraint(lessThanOrEqualTo: widthAnchor, constant: -24).isActive = true
            bottomLeftPlacement = [view.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
                                   view.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -12)]
            caretLeft = view.leftAnchor.constraint(equalTo: leftAnchor)
            caretTop = view.topAnchor.constraint(equalTo: topAnchor)
            preeditView = view
        }
        preeditView?.stringValue = composition.text
        preeditView?.isHidden = !hasMarkedText()
        placePreedit()
        inputContext?.invalidateCharacterCoordinates()
    }

    /// Just below the guest caret, clamped inside the view (HvfGuestCaret.overlayOrigin), or the bottom-left.
    private func placePreedit() {
        guard let view = preeditView, let caretLeft, let caretTop else { return }
        guard let caret = compositionCaret, hasMarkedText() else {
            NSLayoutConstraint.deactivate([caretLeft, caretTop])
            NSLayoutConstraint.activate(bottomLeftPlacement)
            return
        }
        let top = isFlipped ? caret.minY : bounds.height - caret.maxY
        let size = view.intrinsicContentSize
        let origin = HvfGuestCaret.overlayOrigin(
            caret: NSRect(x: caret.minX, y: top, width: caret.width, height: caret.height),
            overlay: NSSize(width: max(0, min(size.width, bounds.width - 24)), height: size.height), view: bounds.size)
        caretLeft.constant = origin.x
        caretTop.constant = origin.y
        NSLayoutConstraint.deactivate(bottomLeftPlacement)
        NSLayoutConstraint.activate([caretLeft, caretTop])
    }

    /// One question per composition; an answer counts only while text is still composed and nothing made it stale.
    private func askCaret() {
        let epoch = caretEpoch
        _ = askGuestCaret { [weak self] caret in
            guard let self, self.caretEpoch == epoch, self.hasMarkedText() else { return }
            self.compositionCaret = caret
            self.refreshPreedit()
        }
    }

    private func forgetCaret() {
        caretEpoch &+= 1
        guard compositionCaret != nil else { return }
        compositionCaret = nil
        placePreedit()
        inputContext?.invalidateCharacterCoordinates()
    }

    private static func string(_ value: Any) -> String? {
        (value as? NSAttributedString)?.string ?? value as? String
    }
}

#endif
