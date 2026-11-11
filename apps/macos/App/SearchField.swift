import AppKit
import SwiftUI

struct SearchField: NSViewRepresentable {
    let placeholder: String
    @Binding var text: String
    let identifier: String
    var submit: () -> Void = {}
    var moveSelection: ((Int) -> Void)?
    var cancel: (() -> Void)?

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSSearchField {
        let field = NSSearchField()
        field.sendsSearchStringImmediately = true
        field.target = context.coordinator
        field.action = #selector(Coordinator.changed(_:))
        field.delegate = context.coordinator
        field.setAccessibilityLabel(placeholder)
        field.setAccessibilityIdentifier(identifier)
        return field
    }

    func updateNSView(_ field: NSSearchField, context: Context) {
        context.coordinator.parent = self
        if field.isEnabled != context.environment.isEnabled { field.isEnabled = context.environment.isEnabled }
        field.placeholderString = placeholder
        field.setAccessibilityLabel(placeholder)
        if field.currentEditor() == nil || context.coordinator.shown != text {
            if field.stringValue != text { field.stringValue = text }
            context.coordinator.shown = text
        }
    }

    @MainActor final class Coordinator: NSObject, NSSearchFieldDelegate {
        var parent: SearchField
        var shown: String?
        init(_ parent: SearchField) { self.parent = parent }
        @objc func changed(_ field: NSSearchField) {
            let value = field.currentEditor()?.string ?? field.stringValue
            shown = value
            parent.text = value
        }
        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            if selector == #selector(NSResponder.cancelOperation(_:)), let cancel = parent.cancel {
                cancel()
                return true
            }
            if let moveSelection = parent.moveSelection {
                if selector == #selector(NSResponder.moveDown(_:)) { moveSelection(1); return true }
                if selector == #selector(NSResponder.moveUp(_:)) { moveSelection(-1); return true }
            }
            guard selector == #selector(NSResponder.insertNewline(_:)) else { return false }
            parent.submit()
            return true
        }
    }
}
