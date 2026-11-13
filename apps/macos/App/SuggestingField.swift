import AppKit
import SwiftUI

/// Native free entry with reusable suggestions; the library canonicalizes the value.
struct SuggestingField: NSViewRepresentable {
    let label: String
    let identifier: String
    @Binding var text: String
    let choices: [String]
    let commit: () -> Void
    let focused: () -> Void
    var focusSerial: Int?
    /// Shown in red inside the empty field after a refused save.
    var problem: String? = nil

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSComboBox {
        let box = SuggestionBox()
        box.focused = { [weak coordinator = context.coordinator] in coordinator?.parent.focused() }
        box.completes = true
        box.numberOfVisibleItems = 14
        box.usesDataSource = false
        box.delegate = context.coordinator
        box.setAccessibilityLabel(label)
        box.setAccessibilityIdentifier(identifier)
        return box
    }

    func updateNSView(_ box: NSComboBox, context: Context) {
        let coordinator = context.coordinator
        coordinator.parent = self
        box.isEnabled = context.environment.isEnabled
        let all = choices
        if coordinator.all != all {
            coordinator.all = all
            coordinator.offer(all, in: box)
        }
        // While editing, only a change made elsewhere, such as Undo, replaces the text.
        if box.currentEditor() == nil || coordinator.shown != text {
            if box.stringValue != text { box.stringValue = text }
            coordinator.shown = text
        }
        if !box.isEnabled {
            // Its own color keeps the value legible while the field is disabled.
            let attributes: [NSAttributedString.Key: Any] = [.foregroundColor: NSColor.labelColor,
                                                             .font: box.font ?? .systemFont(ofSize: 0)]
            box.attributedStringValue = NSAttributedString(string: text, attributes: attributes)
        }
        if coordinator.problem != problem {
            coordinator.problem = problem
            box.placeholderAttributedString = problem.map {
                NSAttributedString(string: $0, attributes: [.foregroundColor: NSColor.systemRed, .font: box.font ?? .systemFont(ofSize: 0)])
            }
        }
        if box.isEnabled, let focusSerial, focusSerial != coordinator.focusSerial {
            coordinator.focusSerial = focusSerial
            DispatchQueue.main.async { [weak box, weak coordinator] in
                guard let box, let coordinator, box.isEnabled,
                      coordinator.parent.focusSerial == focusSerial else { return }
                if box.currentEditor() == nil { box.window?.makeFirstResponder(box) }
            }
        }
    }

    @MainActor final class Coordinator: NSObject, NSComboBoxDelegate {
        var parent: SuggestingField
        var all: [String] = []
        var items: [String] = []
        var focusSerial: Int?
        /// The draft's text as this field last showed or sent it.
        var shown: String?
        var problem: String?

        init(_ parent: SuggestingField) { self.parent = parent }

        func offer(_ offered: [String], in box: NSComboBox) {
            guard items != offered else { return }
            items = offered
            box.removeAllItems()
            box.addItems(withObjectValues: offered)
            box.reloadData()
        }

        private func send(_ value: String) {
            shown = value
            parent.text = value
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let box = notification.object as? NSComboBox else { return }
            send(box.stringValue)
        }

        func controlTextDidBeginEditing(_ notification: Notification) { parent.focused() }

        func comboBoxSelectionDidChange(_ notification: Notification) {
            guard let box = notification.object as? NSComboBox else { return }
            let index = box.indexOfSelectedItem
            guard items.indices.contains(index) else { return }
            parent.focused()
            send(items[index])
            parent.commit()
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            guard let box = notification.object as? NSComboBox else { return }
            send(box.stringValue)
            parent.commit()
        }
    }
}

@MainActor private final class SuggestionBox: NSComboBox {
    var focused: (() -> Void)?

    override func becomeFirstResponder() -> Bool {
        let accepted = super.becomeFirstResponder()
        if accepted { focused?() }
        return accepted
    }
}
