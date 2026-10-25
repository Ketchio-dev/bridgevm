import AppKit
import SwiftUI

/// The library search text, as an AppKit text field whose own accessibility element carries
/// the identifier and reports an accessibility value set to the query.
///
/// SwiftUI's TextField exposes a separate accessibility node and updates its binding only from
/// edits made through its field editor, so the d6 run of 2026-10-23 at seal 8d3eb052, whose
/// accessibility client set the value, changed the text on screen but never the query.
struct LibrarySearchTextField: NSViewRepresentable {
    static let identifier = "bridgevm.library.search"
    @Binding var query: String
    /// Each increment makes the field the first responder (the hidden ⌘F button).
    let focusRequests: Int

    func makeCoordinator() -> Coordinator { Coordinator(query: $query, focusRequests: focusRequests) }

    func makeNSView(context: Context) -> LibrarySearchNSTextField {
        let field = LibrarySearchNSTextField()
        field.placeholderString = "VM 이름 또는 엔진 검색"
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.cell?.usesSingleLineMode = true
        field.cell?.isScrollable = true
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        field.cell?.setAccessibilityIdentifier(Self.identifier) // the cell is the element; only it carries the identifier
        field.toolTip = "가상 머신 검색 (⌘F)"
        field.stringValue = query
        field.delegate = context.coordinator
        (field.cell as? LibrarySearchFieldCell)?.report = { [weak coordinator = context.coordinator] in
            coordinator?.changed($0)
        }
        return field
    }

    func updateNSView(_ field: LibrarySearchNSTextField, context: Context) {
        context.coordinator.query = $query
        if field.stringValue != query { field.stringValue = query }
        guard context.coordinator.focusRequests != focusRequests else { return }
        context.coordinator.focusRequests = focusRequests
        field.window?.makeFirstResponder(field)
    }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var query: Binding<String>
        var focusRequests: Int
        init(query: Binding<String>, focusRequests: Int) { self.query = query; self.focusRequests = focusRequests }
        func changed(_ text: String) { if query.wrappedValue != text { query.wrappedValue = text } }
        func controlTextDidChange(_ notification: Notification) {
            if let field = notification.object as? NSTextField { changed(field.stringValue) }
        }
    }
}

/// An AppKit text field is accessible through its cell; a set on the control goes there too.
final class LibrarySearchNSTextField: NSTextField {
    override class var cellClass: AnyClass? {
        get { LibrarySearchFieldCell.self }
        set {}
    }

    override func setAccessibilityValue(_ value: Any?) {
        guard let cell = cell as? LibrarySearchFieldCell else { return super.setAccessibilityValue(value) }
        cell.setAccessibilityValue(value)
    }
}

/// AppKit applies an accessibility value set without a text-change notification; this cell
/// applies it as the field's text and reports it, through either entry point a client uses.
final class LibrarySearchFieldCell: NSTextFieldCell {
    var report: ((String) -> Void)?

    override func setAccessibilityValue(_ value: Any?) {
        guard let text = value as? String else { return super.setAccessibilityValue(value) }
        if let field = controlView as? NSTextField { field.stringValue = text } else { stringValue = text }
        report?(text)
    }

    override func accessibilitySetValue(_ value: Any?, forAttribute attribute: NSAccessibility.Attribute) {
        guard attribute == .value, let text = value as? String else {
            return super.accessibilitySetValue(value, forAttribute: attribute)
        }
        setAccessibilityValue(text)
    }
}
