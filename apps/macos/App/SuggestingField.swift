import AppKit
import SwiftUI

/// Native free entry with reusable suggestions; the library canonicalizes the value.
struct SuggestingField: NSViewRepresentable {
    let label: String
    let identifier: String
    @Binding var text: String
    let choices: [String]
    var focusSerial: Int?
    /// Shown in red inside the empty field after a refused save.
    var problem: String? = nil

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSComboBox {
        let box = NSComboBox()
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
        if coordinator.problem != problem {
            coordinator.problem = problem
            box.placeholderAttributedString = problem.map {
                NSAttributedString(string: $0, attributes: [.foregroundColor: NSColor.systemRed, .font: box.font ?? .systemFont(ofSize: 0)])
            }
        }
        if let focusSerial, focusSerial != coordinator.focusSerial {
            coordinator.focusSerial = focusSerial
            DispatchQueue.main.async { box.window?.makeFirstResponder(box) }
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

        func comboBoxSelectionDidChange(_ notification: Notification) {
            guard let box = notification.object as? NSComboBox else { return }
            let index = box.indexOfSelectedItem
            guard items.indices.contains(index) else { return }
            send(items[index])
        }
    }
}
