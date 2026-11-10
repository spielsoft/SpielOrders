import AppKit
import SwiftUI

/// A text field that offers values to choose from, led by optional actions such as
/// New Vendor…. Typed text reaches the draft as typed; the library matches it. A
/// restricted field is a selector instead: typing filters the offered choices, and
/// leaving the field keeps only a choice, so free text never reaches the draft.
struct SuggestingField: NSViewRepresentable {
    let label: String
    let identifier: String
    @Binding var text: String
    var actions: [String] = []
    let choices: [String]
    var restricted = false
    var focusSerial: Int?
    /// Shown in red inside the empty field after a refused save.
    var problem: String? = nil
    var onAction: (Int) -> Void = { _ in }

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
        let all = actions + choices
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

        /// The text the user typed, without the completion the field proposes after it.
        private func typed(_ box: NSComboBox) -> String {
            guard let editor = box.currentEditor() else { return box.stringValue }
            let text = editor.string as NSString
            let selected = editor.selectedRange
            guard selected.length > 0, NSMaxRange(selected) == text.length else { return editor.string }
            return text.substring(to: selected.location)
        }

        private func matching(_ typed: String) -> [String] {
            let query = typed.trimmingCharacters(in: .whitespaces)
            return query.isEmpty ? parent.choices : parent.choices.filter { $0.localizedCaseInsensitiveContains(query) }
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let box = notification.object as? NSComboBox else { return }
            guard parent.restricted else { return send(box.stringValue) }
            offer(parent.actions + matching(typed(box)), in: box)
            box.cell?.setAccessibilityExpanded(true)
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            guard parent.restricted, let box = notification.object as? NSComboBox else { return }
            let entered = box.stringValue.trimmingCharacters(in: .whitespaces)
            box.cell?.setAccessibilityExpanded(false)
            offer(all, in: box)
            // An unedited field keeps its value, such as a Vendor hidden since the Order chose it.
            if entered == parent.text.trimmingCharacters(in: .whitespaces) { return }
            if entered.isEmpty { return send("") }
            let matches = matching(entered)
            let choice = parent.choices.first { $0 == entered }
                ?? parent.choices.first { $0.localizedCaseInsensitiveCompare(entered) == .orderedSame }
                ?? (matches.count == 1 ? matches[0] : nil)
            if let choice {
                box.stringValue = choice
                if choice != parent.text { send(choice) }
                return
            }
            box.stringValue = parent.text
            if !parent.actions.contains(entered) { NSSound.beep() }
        }

        func comboBoxSelectionDidChange(_ notification: Notification) {
            guard let box = notification.object as? NSComboBox else { return }
            let index = box.indexOfSelectedItem
            guard items.indices.contains(index) else { return }
            if index < parent.actions.count {
                box.deselectItem(at: index)
                // The selection is applied after this notification; restore the draft's text then.
                parent.onAction(index)
                DispatchQueue.main.async { box.stringValue = self.parent.text }
            } else {
                send(items[index])
            }
        }
    }
}
