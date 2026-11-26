import AppKit
import SwiftUI

/// The library search text, as an AppKit text field whose own accessibility element carries
/// the identifier and reports an accessibility value set to the query.
///
/// SwiftUI's TextField exposes a separate accessibility node and updates its binding only from
/// edits made through its field editor, so an accessibility client that set the value changed
/// the text on screen but never the query.
struct LibrarySearchTextField: NSViewRepresentable {
    static let identifier = "bridgevm.library.search"
    @Binding var query: String
    /// Each increment makes the field the first responder (the hidden ⌘F button).
    let focusRequests: Int

    func makeCoordinator() -> Coordinator { Coordinator(query: $query, focusRequests: focusRequests) }

    func makeNSView(context: Context) -> LibrarySearchNSTextField {
        let field = LibrarySearchNSTextField()
        field.placeholderString = "Search by VM name, ID, or engine"
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.cell?.usesSingleLineMode = true
        field.cell?.isScrollable = true
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        field.cell?.setAccessibilityIdentifier(Self.identifier) // the cell is the element; only it carries the identifier
        field.toolTip = "Search virtual machines (⌘F)"
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

    /// The deprecated entry point stays: NSTextFieldCell's own implementation sets the text without calling
    /// setAccessibilityValue(_:) (checked in process against the macOS 27 SDK), so the query would not follow.
    override func accessibilitySetValue(_ value: Any?, forAttribute attribute: NSAccessibility.Attribute) {
        guard attribute == .value, let text = value as? String else {
            return super.accessibilitySetValue(value, forAttribute: attribute)
        }
        setAccessibilityValue(text)
    }
}
