import AppKit
import SwiftUI

/// Handle palette movement only after the native input context interprets the editing command.
struct PaletteSearchField: NSViewRepresentable {
    @Binding var text: String
    let focusRequest: Int
    let move: (Bool) -> Void
    let submit: () -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField()
        field.isBordered = false
        field.drawsBackground = false
        field.cell?.usesSingleLineMode = true
        field.focusRingType = .none
        field.font = .preferredFont(forTextStyle: .title3)
        field.placeholderString = "Search commands or VMs"
        field.setAccessibilityLabel("Search commands or VMs")
        field.setAccessibilityIdentifier("bridgevm.palette.search")
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        field.delegate = context.coordinator
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        let coordinator = context.coordinator
        coordinator.parent = self
        if (field.currentEditor() as? NSTextView)?.hasMarkedText() != true && field.stringValue != text {
            field.stringValue = text
        }
        if coordinator.focusRequest != focusRequest {
            coordinator.focusRequest = focusRequest
            DispatchQueue.main.async { [weak field] in
                guard let field else { return }
                field.window?.makeFirstResponder(field)
            }
        }
    }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: PaletteSearchField
        var focusRequest: Int?
        init(_ parent: PaletteSearchField) { self.parent = parent }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField,
                  (field.currentEditor() as? NSTextView)?.hasMarkedText() != true else { return }
            parent.text = field.stringValue
        }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            guard !textView.hasMarkedText() else { return false }
            switch selector {
            case #selector(NSResponder.moveUp(_:)), #selector(NSResponder.moveDown(_:)):
                parent.text = textView.string
                parent.move(selector == #selector(NSResponder.moveDown(_:)))
            case #selector(NSResponder.insertNewline(_:)):
                parent.text = textView.string
                parent.submit()
            default: return false
            }
            return true
        }
    }
}
